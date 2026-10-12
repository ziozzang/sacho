use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use sacho::{
    config::Config,
    db::{self, CheckSpec, Item, Observation},
    server::{App, router},
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
fn spec() -> CheckSpec {
    CheckSpec {
        name: "Gateway".into(),
        target: "http://127.0.0.1:8080/health".into(),
        ..Default::default()
    }
}
fn observation(status: &str) -> Observation {
    Observation {
        status: status.into(),
        latency_ms: 25,
        output: "{\"version\":\"v1.2.3\",\"tps\":45.2}".into(),
        detail: if status == "outage" {
            "unhealthy".into()
        } else {
            String::new()
        },
    }
}
#[test]
fn incident_hysteresis_deduplicates_and_queues_each_transition_once() -> anyhow::Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let mut spec = spec();
    spec.failure_threshold = 2;
    spec.recovery_threshold = 2;
    db::save(&mut conn, None, &spec)?;
    conn.execute(
        "INSERT INTO webhooks(id,name,url) VALUES('hook','alerts','http://localhost/hook')",
        [],
    )?;
    conn.execute("INSERT INTO subscribers(id,email,token,verified,created_at) VALUES('a','a@example.com','token',1,0)",[])?;
    let item = db::items(&conn)?.remove(0);
    for state in [
        "operational",
        "outage",
        "outage",
        "outage",
        "degraded",
        "operational",
    ] {
        db::record(&mut conn, &item, &observation(state))?;
    }
    assert_eq!(db::items(&conn)?[0].status, "degraded");
    assert_eq!(db::incidents(&conn, 100)?.len(), 1);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    db::record(&mut conn, &item, &observation("operational"))?;
    db::record(&mut conn, &item, &observation("operational"))?;
    assert_eq!(db::items(&conn)?[0].status, "operational");
    assert!(db::incidents(&conn, 100)?[0]["resolved_at"].is_number());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        4
    );
    Ok(())
}
#[test]
fn stale_revision_cannot_overwrite_edited_check() -> anyhow::Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let id = db::save(&mut conn, None, &spec())?;
    let item = db::items(&conn)?.remove(0);
    db::save(&mut conn, Some(id), &spec())?;
    db::record(&mut conn, &item, &observation("outage"))?;
    assert!(db::incidents(&conn, 10)?.is_empty());
    assert!(db::items(&conn)?[0].last_check.is_none());
    Ok(())
}
#[test]
fn display_strings_do_not_determine_health_and_are_opt_in() {
    let mut spec = spec();
    spec.display_pointer = "/version".into();
    assert_eq!(
        db::display_value(&spec, &observation("operational").output),
        None
    );
    spec.publish_value = true;
    assert_eq!(
        db::display_value(&spec, &observation("operational").output),
        Some("v1.2.3".into())
    );
    spec.display_pointer.clear();
    assert_eq!(
        db::display_value(&spec, "hello\nworld"),
        Some("helloworld".into())
    );
    assert_eq!(db::display_value(&spec, "{\"secret\":\"private\"}"), None);
}
#[test]
fn configuration_rejects_unsafe_listener_and_invalid_conditions() {
    let mut cfg = Config {
        bind: "0.0.0.0:8080".into(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
    cfg.auth.mode = "auth-proxy".into();
    assert!(cfg.validate().is_err());
    cfg.auth.admins = vec!["admin@example.com".into()];
    assert!(cfg.validate().is_ok());
    let mut spec = spec();
    spec.timeout_seconds = 120;
    assert!(spec.validate().is_err());
    spec.timeout_seconds = 10;
    spec.interval_seconds = 30;
    assert!(spec.validate().is_ok());
}
fn app(proxy: bool) -> App {
    let mut cfg = Config::default();
    if proxy {
        cfg.auth.mode = "auth-proxy".into();
        cfg.auth.admins = vec!["admin@example.com".into()];
    }
    App {
        db: db::open(":memory:").unwrap(),
        cfg: Arc::new(cfg),
        test_slots: Arc::new(tokio::sync::Semaphore::new(2)),
    }
}
#[tokio::test]
async fn proxy_identity_csrf_and_public_secrets_boundary() -> anyhow::Result<()> {
    let app = app(true);
    let router = router(app.clone());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/items")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/items")
                .header("x-forwarded-email", "other@example.com")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/items")
                .header("x-forwarded-email", "admin@example.com")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = serde_json::to_vec(&spec())?;
    let request = |csrf: bool, origin: &str| {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/admin/items")
            .header("x-forwarded-email", "admin@example.com")
            .header("Content-Type", "application/json")
            .header("Origin", origin);
        if csrf {
            b = b.header("X-Sacho-Request", "1");
        }
        b.body(Body::from(payload.clone())).unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request(false, "http://127.0.0.1:8080"))
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(true, "https://evil.example"))
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(true, "http://127.0.0.1:8080"))
            .await?
            .status(),
        StatusCode::OK
    );
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/api/status").body(Body::empty())?)
        .await?;
    let bytes = to_bytes(response.into_body(), 100000).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert!(!text.contains("target"));
    assert!(!text.contains("http://127.0.0.1:8080/health"));
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty())?)
        .await?;
    let csp = response.headers()["content-security-policy"]
        .to_str()?
        .to_string();
    assert!(csp.contains("nonce-"));
    assert!(!csp.contains("unsafe-inline"));
    Ok(())
}
#[tokio::test]
async fn history_buckets_keep_no_data_and_hide_private_numeric_values() -> anyhow::Result<()> {
    let app = app(false);
    let id = {
        let mut conn = app.db.lock().unwrap();
        let id = db::save(
            &mut conn,
            None,
            &CheckSpec {
                publish_value: true,
                ..spec()
            },
        )?;
        let item = db::items(&conn)?.remove(0);
        let mut obs = observation("operational");
        obs.output = "42".into();
        db::record(&mut conn, &item, &obs)?;
        id
    };
    let router = router(app);
    let to = db::now() + 1;
    let request = Request::builder()
        .uri(format!(
            "/api/history/{id}?from={}&to={to}&precision=30",
            to - 3600
        ))
        .body(Body::empty())?;
    let response = router.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100000).await?)?;
    assert_eq!(data["buckets"].as_array().unwrap().len(), 1);
    assert!(data["buckets"][0]["metric_value"].is_null());
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/history/{id}?precision=30"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}
#[tokio::test]
async fn layout_validates_and_persists_order() -> anyhow::Result<()> {
    let app = app(false);
    let id = db::save(&mut app.db.lock().unwrap(), None, &spec())?;
    let router = router(app);
    for (ids, expected) in [
        (vec![id.clone(), id.clone()], StatusCode::BAD_REQUEST),
        (vec!["missing".into()], StatusCode::BAD_REQUEST),
        (vec![id], StatusCode::OK),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/admin/layout")
                    .header("Content-Type", "application/json")
                    .header("X-Sacho-Request", "1")
                    .body(Body::from(
                        json!({"sections":[{"title":"Infrastructure","item_ids":ids}]}).to_string(),
                    ))?,
            )
            .await?;
        assert_eq!(response.status(), expected);
    }
    let response = router
        .oneshot(Request::builder().uri("/api/status").body(Body::empty())?)
        .await?;
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100000).await?)?;
    assert_eq!(data["site"]["layout"][0]["title"], "Infrastructure");
    Ok(())
}
#[tokio::test]
async fn shell_timeout_kills_descendants_and_python_numeric_condition() -> anyhow::Result<()> {
    let mut spec = spec();
    spec.kind = "shell".into();
    spec.target = "sleep 30 & wait".into();
    spec.timeout_seconds = 1;
    let mut item = Item {
        id: "test".into(),
        spec,
        status: "unknown".into(),
        last_check: None,
        latency_ms: None,
        failures: 0,
        successes: 0,
        revision: 1,
    };
    let start = std::time::Instant::now();
    let obs = sacho::engine::probe(&item).await;
    assert_eq!(obs.status, "outage");
    assert!(start.elapsed().as_secs() < 3);
    item.spec.kind = "python".into();
    item.spec.target = "print('{\"tps\":45.2}')".into();
    item.spec.threshold = Some(20.0);
    item.spec.comparison = "gt".into();
    item.spec.json_pointer = "/tps".into();
    item.spec.timeout_seconds = 5;
    let obs = sacho::engine::probe(&item).await;
    assert_eq!(obs.status, "operational", "{}", obs.detail);
    item.spec.threshold = Some(50.0);
    assert_eq!(sacho::engine::probe(&item).await.status, "outage");
    item.spec.failure_status = "degraded".into();
    let warning = sacho::engine::probe(&item).await;
    assert_eq!(warning.status, "degraded");
    assert!(warning.detail.contains("numeric condition failed"));
    item.spec.target = "raise RuntimeError(\"private error\")".into();
    assert_eq!(sacho::engine::probe(&item).await.status, "degraded");
    Ok(())
}
#[test]
fn lua_runtime_exposes_only_bounded_sacho_api() -> anyhow::Result<()> {
    sacho::engine::lua_probe(
        "assert(os == nil and io == nil and package == nil); assert(sacho.api_version == 1); local d=sacho.json_decode('{\"tps\":12}'); return sacho.json_encode(d)",
    )?;
    assert!(sacho::engine::lua_probe("return false").is_err());
    assert!(sacho::engine::lua_probe("return sacho.env('GITHUB_TOKEN')").is_err());
    Ok(())
}

#[tokio::test]
async fn account_reports_only_verified_proxy_identity_subscription() -> anyhow::Result<()> {
    let app = app(true);
    app.db.lock().unwrap().execute("INSERT INTO subscribers(id,email,token,verified,created_at) VALUES('a','reader@example.com','private-token',0,0)", [])?;
    let router = router(app.clone());
    for (identity, expected) in [
        (None, None),
        (Some("reader@example.com"), Some("pending")),
        (Some("other@example.com"), Some("unsubscribed")),
    ] {
        let mut request = Request::builder().uri("/api/account");
        if let Some(email) = identity {
            request = request.header("X-Forwarded-Email", email);
        }
        let response = router.clone().oneshot(request.body(Body::empty())?).await?;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await?)?;
        assert_eq!(body["email"].as_str(), identity);
        assert_eq!(body["subscription"].as_str(), expected);
        assert!(!body.to_string().contains("private-token"));
    }
    app.db
        .lock()
        .unwrap()
        .execute("UPDATE subscribers SET verified=1", [])?;
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/account")
                .header("X-Forwarded-Email", "reader@example.com")
                .body(Body::empty())?,
        )
        .await?;
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await?)?;
    assert_eq!(body["subscription"], "subscribed");
    Ok(())
}

#[tokio::test]
async fn public_value_survives_failed_check_without_leaking_logs() -> anyhow::Result<()> {
    let app = app(true);
    {
        let mut conn = app.db.lock().unwrap();
        let configured = CheckSpec {
            publish_value: true,
            display_pointer: "/value".into(),
            public_message_pointer: "/message".into(),
            ..spec()
        };
        db::save(&mut conn, None, &configured)?;
        let item = db::items(&conn)?.remove(0);
        let mut success = observation("operational");
        success.output = "{\"value\":20}".into();
        db::record(&mut conn, &item, &success)?;
        let item = db::items(&conn)?.remove(0);
        let mut failed = observation("outage");
        failed.output = "{\"message\":\"Allowance query requires authentication\",\"private\":\"secret-token\"}".into();
        db::record(&mut conn, &item, &failed)?;
    }
    let response = router(app)
        .oneshot(Request::builder().uri("/api/status").body(Body::empty())?)
        .await?;
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
    let item = &data["items"][0];
    assert_eq!(item["status"], "outage");
    assert_eq!(item["display_value"], "20");
    assert_eq!(item["display_value_stale"], true);
    assert!(item["display_value_at"].is_number());
    assert_eq!(
        item["public_message"],
        "Allowance query requires authentication"
    );
    assert!(!data.to_string().contains("secret-token"));
    Ok(())
}

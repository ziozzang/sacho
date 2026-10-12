use sacho::{
    db::{self, CheckSpec, Item, Observation},
    monitoring::{self, ValueRules},
};
use serde_json::json;
fn spec() -> CheckSpec {
    CheckSpec {
        name: "Quota".into(),
        kind: "lua".into(),
        target: "return 'test'".into(),
        publish_value: true,
        display_pointer: "/remaining".into(),
        publish_trend: true,
        trend_pointer: "/remaining".into(),
        value_rules: Some(ValueRules {
            pointer: "/remaining".into(),
            direction: "low".into(),
            warning: Some(20.0),
            critical: Some(5.0),
        }),
        ..Default::default()
    }
}
#[tokio::test]
async fn scalar_thresholds_are_ordered_inclusive_and_preserve_the_value() -> anyhow::Result<()> {
    let db = db::open(":memory:")?;
    let mut item = {
        let mut conn = db.lock().unwrap();
        db::save(&mut conn, None, &spec())?;
        db::items(&conn)?.remove(0)
    };
    item.spec.kind = "python".into();
    for (value, status) in [
        (80, "operational"),
        (20, "degraded"),
        (5, "outage"),
        (0, "outage"),
    ] {
        item.spec.target = format!("print('{{\"remaining\":{value}}}')");
        let obs = sacho::engine::probe(&item).await;
        assert_eq!(obs.status, status);
        assert_eq!(
            db::display_value(&item.spec, &obs.output),
            Some(value.to_string())
        );
    }
    let mut bad = spec();
    bad.value_rules.as_mut().unwrap().critical = Some(30.0);
    assert!(bad.validate().is_err());
    bad.value_rules.as_mut().unwrap().direction = "high".into();
    assert!(bad.validate().is_ok());
    item.spec.target = "print('{\"message\":\"no numeric sample\"}')".into();
    item.spec.failure_status = "degraded".into();
    let obs = sacho::engine::probe(&item).await;
    assert_eq!(obs.status, "degraded");
    assert!(obs.detail.contains("finite JSON number"));
    Ok(())
}
fn record(conn: &mut rusqlite::Connection, item: &Item, status: &str) -> anyhow::Result<()> {
    db::record(
        conn,
        item,
        &Observation {
            status: status.into(),
            latency_ms: 10,
            output: "{\"remaining\":20}".into(),
            detail: String::new(),
        },
    )
}
#[test]
fn debounce_suppresses_short_pairs_but_never_confirmation_mail() -> anyhow::Result<()> {
    let database = db::open(":memory:")?;
    let mut c = database.lock().unwrap();
    db::save(&mut c, None, &spec())?;
    c.execute(
        "INSERT INTO webhooks(id,name,url) VALUES('h','fixture','http://localhost/hook')",
        [],
    )?;
    c.execute(
        "INSERT INTO settings VALUES('notification_policy',?)",
        [json!({"settle_seconds":30,"cooldown_seconds":60}).to_string()],
    )?;
    let item = db::items(&c)?.remove(0);
    record(&mut c, &item, "outage")?;
    let (id, text, next): (i64, String, i64) =
        c.query_row("SELECT id,payload,next_at FROM outbox", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    assert!(next >= db::now() + 29);
    record(&mut c, &item, "operational")?;
    assert!(!monitoring::permit_delivery(
        &c,
        id,
        "webhook",
        "http://localhost/hook",
        &serde_json::from_str(&text)?
    )?);
    let (rid, recovery): (i64, String) =
        c.query_row("SELECT id,payload FROM outbox WHERE id != ?", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    assert!(!monitoring::permit_delivery(
        &c,
        rid,
        "webhook",
        "http://localhost/hook",
        &serde_json::from_str(&recovery)?
    )?);
    c.execute(
        "UPDATE settings SET value=? WHERE key='notification_policy'",
        [json!({"maintenance_until":db::now()+600}).to_string()],
    )?;
    assert!(monitoring::permit_delivery(
        &c,
        999,
        "email",
        "reader@example.com",
        &json!({"event":"subscription.verify"})
    )?);
    let before: i64 = c.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?;
    record(&mut c, &item, "outage")?;
    assert_eq!(
        before + 1,
        c.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?
    );
    assert_eq!(db::items(&c)?[0].status, "outage");
    Ok(())
}
#[test]
fn cooldown_and_recovery_delivery_are_per_destination() -> anyhow::Result<()> {
    let database = db::open(":memory:")?;
    let mut c = database.lock().unwrap();
    db::save(&mut c, None, &spec())?;
    c.execute(
        "INSERT INTO webhooks(id,name,url) VALUES('h','fixture','http://localhost/hook')",
        [],
    )?;
    c.execute(
        "INSERT INTO settings VALUES('notification_policy',?)",
        [json!({"cooldown_seconds":300}).to_string()],
    )?;
    let item = db::items(&c)?.remove(0);
    record(&mut c, &item, "outage")?;
    c.execute("UPDATE outbox SET delivered_at=?", [db::now()])?;
    record(&mut c, &item, "operational")?;
    let (id, text): (i64, String) = c.query_row(
        "SELECT id,payload FROM outbox ORDER BY id DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert!(monitoring::permit_delivery(
        &c,
        id,
        "webhook",
        "http://localhost/hook",
        &serde_json::from_str(&text)?
    )?);
    record(&mut c, &item, "outage")?;
    let next: i64 = c.query_row(
        "SELECT next_at FROM outbox ORDER BY id DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    assert!(next >= db::now() + 299);
    Ok(())
}
#[test]
fn heartbeat_and_safe_export_exclude_sources_and_server_secrets() -> anyhow::Result<()> {
    let database = db::open(":memory:")?;
    let mut c = database.lock().unwrap();
    let mut spec = spec();
    spec.target = "return sacho.env('PRIVATE_INPUT') .. 'secret-embedded-token'".into();
    db::save(&mut c, None, &spec)?;
    let cfg = sacho::config::Config::default();
    let export = sacho::setup::export(&c, &cfg)?.to_string();
    assert!(!export.contains("secret-embedded-token"));
    assert!(export.contains("PRIVATE_INPUT"));
    let report = sacho::setup::validate(&spec, &cfg);
    assert_eq!(report["valid"], true);
    assert!(!report["warnings"].as_array().unwrap().is_empty());
    spec.target = "local = broken".into();
    assert_eq!(sacho::setup::validate(&spec, &cfg)["valid"], false);
    assert_eq!(
        monitoring::worker_health(&c, "edge", false)?["connection"],
        "unknown"
    );
    monitoring::heartbeat(&c, "edge", Some("1.2.3"), Some("test-platform"))?;
    let health = monitoring::worker_health(&c, "edge", false)?;
    assert_eq!(health["connection"], "online");
    assert_eq!(health["reported_version"], "1.2.3");
    assert_eq!(
        monitoring::worker_health(&c, "edge", true)?["connection"],
        "revoked"
    );
    Ok(())
}

#[test]
fn delayed_event_uses_current_severity_and_respects_scalar_privacy() -> anyhow::Result<()> {
    let database = db::open(":memory:")?;
    let mut c = database.lock().unwrap();
    let mut configured = spec();
    let id = db::save(&mut c, None, &configured)?;
    let item = db::items(&c)?.remove(0);
    record(&mut c, &item, "degraded")?;
    let incident: String = c.query_row("SELECT id FROM incidents", [], |r| r.get(0))?;
    let mut event = json!({"event":"incident.started","incident_id":incident,"item_id":id,"status":"degraded","display_value":"old"});
    record(&mut c, &item, "outage")?;
    monitoring::refresh_event(&c, &mut event)?;
    assert_eq!(event["status"], "outage");
    assert_eq!(event["display_value"], "20");
    configured.publish_value = false;
    db::save(&mut c, Some(id), &configured)?;
    monitoring::refresh_event(&c, &mut event)?;
    assert!(event["display_value"].is_null());
    Ok(())
}

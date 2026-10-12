use crate::{
    config::Config,
    db::{self, CheckSpec, Db},
};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    extract::Request,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post, put},
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
pub struct App {
    pub db: Db,
    pub cfg: Arc<Config>,
    pub test_slots: Arc<tokio::sync::Semaphore>,
}
#[derive(Debug)]
pub struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::warn!(%error,"API operation failed");
        Self(
            StatusCode::BAD_REQUEST,
            "Operation failed; verify the supplied fields and server configuration.".into(),
        )
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        Self::from(anyhow::Error::from(error))
    }
}
type ApiResult<T> = std::result::Result<T, ApiError>;
fn bad(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
async fn admin_auth(State(app): State<App>, request: Request, next: Next) -> Response {
    let authorized = if app.cfg.auth.mode == "no-auth" {
        true
    } else {
        request
            .headers()
            .get(&app.cfg.auth.email_header)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|email| {
                app.cfg
                    .auth
                    .admins
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(email.trim()))
            })
    };
    if !authorized {
        return ApiError(
            StatusCode::UNAUTHORIZED,
            "Administrator identity required.".into(),
        )
        .into_response();
    }
    if request.method() != axum::http::Method::GET && request.method() != axum::http::Method::HEAD {
        if request
            .headers()
            .get("X-Sacho-Request")
            .and_then(|v| v.to_str().ok())
            != Some("1")
        {
            return ApiError(
                StatusCode::FORBIDDEN,
                "X-Sacho-Request: 1 header required.".into(),
            )
            .into_response();
        }
        if let Some(origin) = request.headers().get(header::ORIGIN)
            && !origin
                .to_str()
                .is_ok_and(|origin| app.cfg.allows_origin(origin))
        {
            return ApiError(StatusCode::FORBIDDEN, "Origin is not allowed.".into())
                .into_response();
        }
    }
    next.run(request).await
}
pub fn router(app: App) -> Router {
    let admin = Router::new()
        .route("/api/admin/items", get(admin_items).post(create_item))
        .route(
            "/api/admin/items/{id}",
            put(update_item).delete(delete_item),
        )
        .route("/api/admin/items/{id}/run", post(run_item))
        .route("/api/admin/items/{id}/logs", get(logs))
        .route("/api/admin/test", post(test_draft))
        .route("/api/admin/validate", post(validate_draft))
        .route("/api/admin/examples", get(examples))
        .route("/api/admin/export", get(export_settings))
        .route(
            "/api/admin/notification-policy",
            get(notification_policy).put(save_notification_policy),
        )
        .route(
            "/api/admin/templates",
            get(get_templates).put(save_templates),
        )
        .route("/api/admin/templates/preview", post(preview_templates))
        .route("/api/admin/layout", put(save_layout))
        .route("/api/admin/settings", get(settings).put(save_settings))
        .route("/api/admin/subscribers", get(subscribers))
        .route("/api/admin/subscribers/{id}", delete(delete_subscriber))
        .route("/api/admin/webhooks", get(webhooks).post(create_webhook))
        .route("/api/admin/webhooks/{id}", delete(delete_webhook))
        .route("/api/admin/notifications", get(notifications))
        .route("/api/admin/workers", get(workers))
        .route("/api/admin/worker-bootstrap", post(create_bootstrap))
        .route("/api/admin/workers/{node}", delete(revoke_worker))
        .route_layer(middleware::from_fn_with_state(app.clone(), admin_auth));
    Router::new().merge(admin).merge(crate::remote::router())
        .route("/",get(index)).route("/admin",get(index)).route("/admin/",get(index))
        .route("/assets/lua-editor.js",get(lua_script)).route("/assets/lua-editor.css",get(lua_style))
        .route("/assets/app.js",get(script)).route("/assets/style.css",get(style))
        .route("/healthz",get(health))
        .route("/api/account",get(account))
        .route("/api/status",get(status)).route("/api/history/{id}",get(history))
        .route("/api/incidents",get(public_incidents))
        .route("/api/subscribe",post(subscribe))
        .route("/subscription/confirm",get(confirm_landing).post(confirm_subscription))
        .route("/subscription/unsubscribe",get(unsubscribe_landing).post(unsubscribe))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(128*1024))
        .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY,axum::http::HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'")))
        .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS,axum::http::HeaderValue::from_static("nosniff")))
        .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY,axum::http::HeaderValue::from_static("no-referrer")))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(header::CACHE_CONTROL,axum::http::HeaderValue::from_static("no-store")))
        .with_state(app)
}
async fn account(State(app): State<App>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let email = if app.cfg.auth.mode == "auth-proxy" {
        headers
            .get(&app.cfg.auth.email_header)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim().to_lowercase())
            .filter(|v| valid_email(v))
    } else {
        None
    };
    let Some(email) = email else {
        return Ok(Json(json!({"email":null})));
    };
    let verified: Option<bool> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT verified FROM subscribers WHERE email=?",
            [&email],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Json(
        json!({"email":email,"subscription":match verified {Some(true)=>"subscribed",Some(false)=>"pending",None=>"unsubscribed"},"logout_url":app.cfg.auth.logout_url}),
    ))
}
async fn index() -> Response {
    let nonce = Uuid::new_v4().simple().to_string();
    let csp = format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'nonce-{nonce}'; img-src 'self' data:; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    );
    (
        [(header::CONTENT_SECURITY_POLICY, csp)],
        Html(include_str!("../web/index.html").replace("__SACHO_NONCE__", &nonce)),
    )
        .into_response()
}
async fn lua_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../web/lua-editor.js"),
    )
}
async fn lua_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/lua-editor.css"),
    )
}
async fn script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../web/app.js"),
    )
}
async fn style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/style.css"),
    )
}
async fn health(State(app): State<App>) -> ApiResult<Json<Value>> {
    app.db
        .lock()
        .unwrap()
        .query_row("SELECT 1", [], |r| r.get::<_, i32>(0))?;
    Ok(Json(json!({"status":"ok","version":crate::VERSION})))
}
async fn status(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut result = vec![];
    for item in db::items(&conn)? {
        if !item.spec.enabled {
            continue;
        }
        let stale = item.last_check.is_none_or(|at| {
            db::now() - at > (item.spec.interval_seconds * 2 + item.spec.timeout_seconds) as i64
        });
        let effective = if stale { "unknown" } else { &item.status };
        let display = if item.spec.publish_value {
            conn.query_row(
                "SELECT display_value,at,id,output FROM checks WHERE item_id=? AND display_value IS NOT NULL AND display_value != '' ORDER BY id DESC LIMIT 1",
                [&item.id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?,r.get::<_, String>(3)?)),
            ).optional()?
        } else {
            None
        };
        let latest = conn
            .query_row(
                "SELECT id,output FROM checks WHERE item_id=? ORDER BY id DESC LIMIT 1",
                [&item.id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let value_stale = display
            .as_ref()
            .is_some_and(|v| stale || latest.as_ref().is_none_or(|c| c.0 != v.2));
        let last_success: Option<i64> = conn.query_row(
            "SELECT MAX(at) FROM checks WHERE item_id=? AND status='operational'",
            [&item.id],
            |r| r.get(0),
        )?;
        let next_check = item
            .last_check
            .map(|at| at + item.spec.interval_seconds as i64)
            .unwrap_or(db::now());
        let value_severity = if item.spec.publish_value {
            item.spec.value_rules.as_ref().and_then(|rules| {
                display
                    .as_ref()
                    .and_then(|v| crate::monitoring::number(&v.3, &rules.pointer))
                    .map(|n| rules.severity(n))
            })
        } else {
            None
        };
        let public_rules = if item.spec.publish_trend {
            item.spec
                .value_rules
                .as_ref()
                .map(|r| json!({"direction":r.direction,"warning":r.warning,"critical":r.critical}))
        } else {
            None
        };
        let message = latest
            .as_ref()
            .and_then(|c| db::public_message(&item.spec, &c.1));
        result.push(json!({"id":item.id,"name":item.spec.name,"description":item.spec.description,"group":item.spec.group,"status":effective,"last_check":item.last_check,"latency_ms":item.latency_ms,"interval_seconds":item.spec.interval_seconds,"last_success":last_success,"next_check":next_check,"freshness":if stale{"stale"}else{"fresh"},"value_severity":value_severity,"value_thresholds":public_rules,"publish_trend":item.spec.publish_trend,"publish_value":item.spec.publish_value,"display_value":display.as_ref().map(|v| &v.0),"display_value_at":display.as_ref().map(|v| v.1),"display_value_stale":value_stale,"public_message":message,"display_label":item.spec.display_label,"metric_unit":item.spec.metric_unit}));
    }
    let status = if result.iter().any(|i| i["status"] == "outage") {
        "outage"
    } else if result.iter().any(|i| i["status"] == "degraded") {
        "degraded"
    } else if result.is_empty() || result.iter().any(|i| i["status"] == "unknown") {
        "unknown"
    } else {
        "operational"
    };
    Ok(Json(
        json!({"site":db::site(&conn,&app.cfg)?,"status":status,"items":result,"updated_at":db::now(),"subscriptions_enabled":app.cfg.smtp.is_some(),"version":crate::VERSION}),
    ))
}
#[derive(Deserialize)]
struct HistoryQuery {
    from: Option<i64>,
    to: Option<i64>,
    precision: Option<i64>,
}
async fn history(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<Value>> {
    let to = q.to.unwrap_or(db::now());
    let from = q.from.unwrap_or(to - 90 * 86400);
    let precision = q.precision.unwrap_or(86400);
    if ![5, 10, 30, 60, 300, 1800, 3600, 86400].contains(&precision)
        || from >= to
        || to > db::now() + 60
        || from < 0
        || to - from > app.cfg.retention_days as i64 * 86400
        || (to - from) / precision > 10000
    {
        return Err(bad("Invalid range or precision; maximum 10,000 buckets."));
    }
    let conn = app.db.lock().unwrap();
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM items WHERE id=? AND json_extract(spec,'$.enabled')=1)",
        [&id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(ApiError(StatusCode::NOT_FOUND, "Service not found.".into()));
    }
    let publish: bool = conn.query_row(
        "SELECT COALESCE(json_extract(spec,'$.publish_trend'),0)=1 FROM items WHERE id=?",
        [&id],
        |r| r.get(0),
    )?;
    let mut stmt=conn.prepare("SELECT (at / ? )*?,COUNT(*),SUM(status='operational'),AVG(latency_ms),MAX(CASE status WHEN 'outage' THEN 2 WHEN 'degraded' THEN 1 ELSE 0 END),CASE WHEN ? THEN AVG(metric_value) ELSE NULL END,CASE WHEN ? THEN MIN(metric_value) ELSE NULL END,CASE WHEN ? THEN MAX(metric_value) ELSE NULL END FROM checks WHERE item_id=? AND at>=? AND at<? GROUP BY at / ? ORDER BY at / ?")?;
    let data=stmt.query_map(params![precision,precision,publish,publish,publish,id,from,to,precision,precision],|r|Ok(json!({"at":r.get::<_,i64>(0)?,"samples":r.get::<_,i64>(1)?,"healthy":r.get::<_,i64>(2)?,"latency_ms":r.get::<_,f64>(3)?,"severity":r.get::<_,i64>(4)?,"metric_value":r.get::<_,Option<f64>>(5)?,"metric_min":r.get::<_,Option<f64>>(6)?,"metric_max":r.get::<_,Option<f64>>(7)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    // Empty buckets are deliberately absent. The UI renders these as no data.
    Ok(Json(
        json!({"from":from,"to":to,"precision":precision,"buckets":data,"uptime_basis":"observed_checks"}),
    ))
}
async fn public_incidents(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut incidents = db::incidents(&conn, 100)?;
    for incident in &mut incidents {
        incident.as_object_mut().unwrap().remove("detail");
    }
    Ok(Json(json!(incidents)))
}
async fn admin_items(State(app): State<App>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(db::items(&app.db.lock().unwrap())?)))
}
async fn workers(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let items = db::items(&conn)?;
    let mut nodes = vec![];
    for worker in &app.cfg.remote_workers {
        let assigned: Vec<_> = items
            .iter()
            .filter(|item| item.spec.worker == worker.id)
            .collect();
        let last_result = assigned.iter().filter_map(|item| item.last_check).max();
        nodes.push(json!({"id":worker.id,"allow_scripts":worker.allow_scripts,"monitors":assigned.len(),"last_result":last_result}));
    }
    for worker in crate::enrollment::nodes(&conn)? {
        if app
            .cfg
            .remote_workers
            .iter()
            .any(|node| node.id == worker.id)
        {
            continue;
        }
        let assigned: Vec<_> = items
            .iter()
            .filter(|item| item.spec.worker == worker.id)
            .collect();
        nodes.push(json!({"id":worker.id,"label":worker.label,"allow_scripts":worker.allow_scripts,"monitors":assigned.len(),"last_result":assigned.iter().filter_map(|item|item.last_check).max(),"registered_at":worker.registered_at,"version":worker.version,"platform":worker.platform,"revoked":worker.revoked,"source":"bootstrap"}));
    }
    for node in &mut nodes {
        let health = crate::monitoring::worker_health(
            &conn,
            node["id"].as_str().unwrap(),
            node["revoked"].as_bool().unwrap_or(false),
        )?;
        node.as_object_mut()
            .unwrap()
            .extend(health.as_object().unwrap().clone());
    }
    Ok(Json(json!(nodes)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapDraft {
    id: String,
    #[serde(default)]
    allow_scripts: bool,
    ttl_seconds: i64,
}
async fn create_bootstrap(
    State(app): State<App>,
    Json(draft): Json<BootstrapDraft>,
) -> ApiResult<Json<Value>> {
    if app
        .cfg
        .remote_workers
        .iter()
        .any(|node| node.id == draft.id)
    {
        return Err(bad("This node uses static configuration"));
    }
    let (key, expires) = crate::enrollment::invite(
        &mut app.db.lock().unwrap(),
        &draft.id,
        draft.allow_scripts,
        draft.ttl_seconds,
    )?;
    Ok(Json(
        json!({"bootstrap_key":key,"node_id":draft.id,"expires_at":expires,"control_url":app.cfg.public_url}),
    ))
}
async fn revoke_worker(State(app): State<App>, Path(node): Path<String>) -> ApiResult<Json<Value>> {
    if app
        .cfg
        .remote_workers
        .iter()
        .any(|worker| worker.id == node)
    {
        return Err(bad(
            "Remove statically configured nodes from server configuration",
        ));
    }
    crate::enrollment::revoke(&mut app.db.lock().unwrap(), &node)?;
    Ok(Json(json!({"revoked":true})))
}
fn validate_worker(conn: &rusqlite::Connection, cfg: &Config, spec: &CheckSpec) -> ApiResult<()> {
    if spec.worker.is_empty() {
        return Ok(());
    }
    let allow_scripts = crate::remote::node_permission(conn, cfg, &spec.worker)?
        .ok_or_else(|| bad("Worker is not registered"))?;
    if spec.kind != "http" && !allow_scripts {
        return Err(bad("This worker is not allowed to execute scripts"));
    }
    Ok(())
}
async fn create_item(
    State(app): State<App>,
    Json(spec): Json<CheckSpec>,
) -> ApiResult<Json<Value>> {
    spec.validate().map_err(|e| bad(&e.to_string()))?;
    if spec.kind == "lua" {
        crate::engine::validate_lua(&spec.target).map_err(|_| bad("Lua syntax is invalid"))?;
    }
    validate_worker(&app.db.lock().unwrap(), &app.cfg, &spec)?;
    let id = db::save(&mut app.db.lock().unwrap(), None, &spec)?;
    Ok(Json(json!({"id":id})))
}
async fn update_item(
    State(app): State<App>,
    Path(id): Path<String>,
    Json(spec): Json<CheckSpec>,
) -> ApiResult<Json<Value>> {
    spec.validate().map_err(|e| bad(&e.to_string()))?;
    if spec.kind == "lua" {
        crate::engine::validate_lua(&spec.target).map_err(|_| bad("Lua syntax is invalid"))?;
    }
    validate_worker(&app.db.lock().unwrap(), &app.cfg, &spec)?;
    db::save(&mut app.db.lock().unwrap(), Some(id), &spec)?;
    Ok(Json(json!({"ok":true})))
}
async fn delete_item(State(app): State<App>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    // An administratively deleted monitor closes its incident without sending a recovery.
    tx.execute("UPDATE incidents SET status='removed',resolved_at=? WHERE item_id=? AND resolved_at IS NULL",params![db::now(),id])?;
    let n = tx.execute("DELETE FROM items WHERE id=?", [id])?;
    tx.commit()?;
    if n == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "Service not found.".into()));
    }
    Ok(Json(json!({"ok":true})))
}
async fn run_item(State(app): State<App>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let n = app
        .db
        .lock()
        .unwrap()
        .execute("UPDATE items SET last_check=NULL WHERE id=?", [id])?;
    if n == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "Service not found.".into()));
    }
    Ok(Json(json!({"queued":true})))
}
#[derive(Deserialize)]
struct LogQuery {
    before: Option<i64>,
    limit: Option<u32>,
}
async fn logs(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut stmt=conn.prepare("SELECT id,at,status,latency_ms,output,detail FROM checks WHERE item_id=? AND id<? ORDER BY id DESC LIMIT ?")?;
    let data=stmt.query_map(params![id,q.before.unwrap_or(i64::MAX),q.limit.unwrap_or(50).clamp(1,200)],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"at":r.get::<_,i64>(1)?,"status":r.get::<_,String>(2)?,"latency_ms":r.get::<_,i64>(3)?,"output":r.get::<_,String>(4)?,"detail":r.get::<_,String>(5)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(Json(json!(data)))
}
async fn settings(State(app): State<App>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"site":db::site(&app.db.lock().unwrap(),&app.cfg)?,"auth":app.cfg.auth,"smtp":app.cfg.smtp,"updates":app.cfg.updates,"version":crate::VERSION,"target":crate::TARGET,"retention_days":app.cfg.retention_days,"external_config":true}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteSettings {
    title: String,
    description: String,
}
async fn save_settings(
    State(app): State<App>,
    Json(site): Json<SiteSettings>,
) -> ApiResult<Json<Value>> {
    if site.title.trim().is_empty() || site.title.len() > 120 || site.description.len() > 2000 {
        return Err(bad("Invalid site title or description."));
    }
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    for (key, value) in [("title", site.title), ("description", site.description)] {
        tx.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
    }
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subscription {
    email: String,
}
fn valid_email(email: &str) -> bool {
    email.len() <= 254
        && !email.contains(['\r', '\n', '<', '>', ' '])
        && email.parse::<lettre::Address>().is_ok()
}
async fn subscribe(
    State(app): State<App>,
    headers: HeaderMap,
    Json(body): Json<Subscription>,
) -> ApiResult<Json<Value>> {
    if app.cfg.smtp.is_none() {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Email subscriptions are not configured.".into(),
        ));
    }
    if headers.get("X-Sacho-Request").and_then(|v| v.to_str().ok()) != Some("1") {
        return Err(bad("X-Sacho-Request: 1 required."));
    }
    if let Some(origin) = headers.get(header::ORIGIN)
        && !origin
            .to_str()
            .is_ok_and(|origin| app.cfg.allows_origin(origin))
    {
        return Err(ApiError(StatusCode::FORBIDDEN, "Origin mismatch.".into()));
    }
    let email = body.email.trim().to_lowercase();
    if !valid_email(&email) {
        return Err(bad("Valid email address required."));
    }
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    let recent:i64=tx.query_row("SELECT COUNT(*) FROM outbox WHERE kind='email' AND created_at>? AND json_extract(payload,'$.event')='subscription.verify'",[db::now()-3600],|r|r.get(0))?;
    if recent >= 100 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Please try again later.".into(),
        ));
    }
    let existing: Option<(String, bool)> = tx
        .query_row(
            "SELECT token,verified FROM subscribers WHERE email=?",
            [&email],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if existing.is_none() {
        let token = Uuid::new_v4().simple().to_string() + &Uuid::new_v4().simple().to_string();
        tx.execute(
            "INSERT INTO subscribers(id,email,token,created_at) VALUES(?,?,?,?)",
            params![Uuid::new_v4().to_string(), email, token, db::now()],
        )?;
        db::queue(
            &tx,
            "email",
            &email,
            &json!({"event":"subscription.verify","token":token}),
        )?;
    } else if let Some((token, false)) = existing {
        let recent: i64 = tx.query_row(
            "SELECT COUNT(*) FROM outbox WHERE destination=? AND created_at>?",
            params![email, db::now() - 3600],
            |r| r.get(0),
        )?;
        if recent == 0 {
            db::queue(
                &tx,
                "email",
                &email,
                &json!({"event":"subscription.verify","token":token}),
            )?;
        }
    }
    tx.commit()?;
    Ok(Json(
        json!({"ok":true,"message":"If confirmation is needed, an email will be sent."}),
    ))
}
#[derive(Deserialize)]
struct Token {
    token: String,
}
fn token_valid(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
fn landing(token: &str, confirm: bool) -> ApiResult<Html<String>> {
    if !token_valid(token) {
        return Err(bad("Invalid subscription link."));
    }
    let (title, action) = if confirm {
        ("Confirm subscription / 구독 확인", "confirm")
    } else {
        ("Unsubscribe / 구독 해지", "unsubscribe")
    };
    Ok(Html(format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><link rel=\"stylesheet\" href=\"/assets/style.css\"><title>{title}</title><main class=\"subscription-page\"><p class=\"eyebrow\">SACHO · SUBSCRIPTIONS</p><h1>{title}</h1><form method=\"post\" action=\"/subscription/{action}?token={token}\"><button class=\"primary\" type=\"submit\">{title}</button></form><p><a href=\"/\">Return to status / 상태 화면으로</a></p></main></html>"
    )))
}
async fn confirm_landing(Query(q): Query<Token>) -> ApiResult<Html<String>> {
    landing(&q.token, true)
}
async fn unsubscribe_landing(Query(q): Query<Token>) -> ApiResult<Html<String>> {
    landing(&q.token, false)
}
async fn confirm_subscription(
    State(app): State<App>,
    Query(q): Query<Token>,
) -> ApiResult<Html<&'static str>> {
    if !token_valid(&q.token) {
        return Err(bad("Invalid subscription link."));
    }
    let n = app.db.lock().unwrap().execute(
        "UPDATE subscribers SET verified=1 WHERE token=? AND (verified=1 OR created_at>?)",
        params![q.token, db::now() - 86400],
    )?;
    if n == 0 {
        return Err(bad("Confirmation expired or subscription not found."));
    }
    Ok(Html(
        "Subscription confirmed / 구독이 확인되었습니다. <a href='/'>Status / 상태</a>",
    ))
}
async fn unsubscribe(
    State(app): State<App>,
    Query(q): Query<Token>,
) -> ApiResult<Html<&'static str>> {
    if !token_valid(&q.token) {
        return Err(bad("Invalid subscription link."));
    }
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    let email: Option<String> = tx
        .query_row(
            "SELECT email FROM subscribers WHERE token=?",
            [&q.token],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(email) = email {
        tx.execute(
            "DELETE FROM outbox WHERE destination=? AND kind='email' AND delivered_at IS NULL",
            [email],
        )?;
    }
    tx.execute("DELETE FROM subscribers WHERE token=?", [q.token])?;
    tx.commit()?;
    Ok(Html(
        "Unsubscribed / 구독이 해지되었습니다. <a href='/'>Status / 상태</a>",
    ))
}
async fn subscribers(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id,email,verified,created_at FROM subscribers ORDER BY created_at DESC")?;
    let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"email":r.get::<_,String>(1)?,"verified":r.get::<_,bool>(2)?,"created_at":r.get::<_,i64>(3)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(Json(json!(rows)))
}
async fn delete_subscriber(
    State(app): State<App>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM outbox WHERE kind='email' AND delivered_at IS NULL AND destination=(SELECT email FROM subscribers WHERE id=?)",[&id])?;
    tx.execute("DELETE FROM subscribers WHERE id=?", [id])?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}
async fn webhooks(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id,name,url,enabled FROM webhooks ORDER BY name")?;
    let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"url":r.get::<_,String>(2)?,"enabled":r.get::<_,bool>(3)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(Json(json!(rows)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Webhook {
    name: String,
    url: String,
}
async fn create_webhook(
    State(app): State<App>,
    Json(hook): Json<Webhook>,
) -> ApiResult<Json<Value>> {
    let url = reqwest::Url::parse(&hook.url).map_err(|_| bad("Invalid webhook URL."))?;
    if hook.name.trim().is_empty()
        || hook.name.len() > 120
        || hook.url.len() > 4096
        || !matches!(url.scheme(), "http" | "https")
        || url.username() != ""
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(bad(
            "Webhook name and HTTP(S) URL required; use HTTPS outside trusted networks.",
        ));
    }
    let id = Uuid::new_v4().to_string();
    app.db.lock().unwrap().execute(
        "INSERT INTO webhooks(id,name,url) VALUES(?,?,?)",
        params![id, hook.name, hook.url],
    )?;
    Ok(Json(json!({"id":id})))
}
async fn delete_webhook(State(app): State<App>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM outbox WHERE kind='webhook' AND delivered_at IS NULL AND destination=(SELECT url FROM webhooks WHERE id=?)",[&id])?;
    tx.execute("DELETE FROM webhooks WHERE id=?", [id])?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}
async fn notifications(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let mut stmt=conn.prepare("SELECT id,kind,destination,attempts,next_at,delivered_at,error,created_at FROM outbox ORDER BY id DESC LIMIT 100")?;
    let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"kind":r.get::<_,String>(1)?,"destination":r.get::<_,String>(2)?,"attempts":r.get::<_,u32>(3)?,"next_at":r.get::<_,i64>(4)?,"delivered_at":r.get::<_,Option<i64>>(5)?,"error":r.get::<_,Option<String>>(6)?,"created_at":r.get::<_,i64>(7)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(Json(json!(rows)))
}
pub async fn serve(cfg: Config, ready_file: Option<PathBuf>) -> Result<()> {
    let mut shutdown = crate::supervisor::shutdown_listener()?;
    cfg.validate()?;
    let db = db::open(&cfg.database)?;
    let cfg = Arc::new(cfg);
    let app = App {
        db: db.clone(),
        cfg: cfg.clone(),
        test_slots: Arc::new(tokio::sync::Semaphore::new(2)),
    };
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let checks = tokio::spawn(crate::engine::scheduler(
        db.clone(),
        cfg.clone(),
        stop_rx.clone(),
    ));
    let notifications = tokio::spawn(crate::notify::worker(db.clone(), cfg.clone(), stop_rx));
    if let Some(path) = &ready_file {
        std::fs::write(path, crate::VERSION.as_bytes())?;
    }
    tracing::info!(bind=%cfg.bind,"Sacho ready");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async move {
            crate::supervisor::cancelled(&mut shutdown).await;
            let _ = stop_tx.send(true);
        })
        .await?;
    let _ = checks.await;
    let _ = notifications.await;
    ensure!(
        db.lock()
            .unwrap()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .is_ok(),
        "database checkpoint failed"
    );
    Ok(())
}

async fn test_draft(State(app): State<App>, Json(spec): Json<CheckSpec>) -> ApiResult<Json<Value>> {
    spec.validate().map_err(|e| bad(&e.to_string()))?;
    if !spec.worker.is_empty() {
        return Err(bad(
            "Remote checks execute after saving; draft tests run on the controller only.",
        ));
    }
    let _permit = app.test_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many draft tests running.".into(),
        )
    })?;
    let item = db::Item {
        id: "draft".into(),
        spec,
        status: "unknown".into(),
        last_check: None,
        latency_ms: None,
        failures: 0,
        successes: 0,
        revision: 1,
    };
    let obs = crate::engine::probe_with_config(&item, &app.cfg).await;
    Ok(Json(
        json!({"status":obs.status,"latency_ms":obs.latency_ms,"detail":obs.detail,"display_value":db::display_value(&item.spec,&obs.output),"validation":crate::setup::validate(&item.spec,&app.cfg),"numeric_value":crate::monitoring::number(&obs.output,crate::monitoring::metric_pointer(&item.spec)),"public_message":db::public_message(&item.spec,&obs.output),"output":obs.output}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    sections: Vec<Section>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Section {
    title: String,
    item_ids: Vec<String>,
}
async fn save_layout(State(app): State<App>, Json(layout): Json<Layout>) -> ApiResult<Json<Value>> {
    if layout.sections.len() > 100 {
        return Err(bad("Too many sections."));
    }
    let conn = app.db.lock().unwrap();
    let known: std::collections::HashSet<_> = db::items(&conn)?.into_iter().map(|i| i.id).collect();
    let mut seen = std::collections::HashSet::new();
    for section in &layout.sections {
        if section.title.trim().is_empty()
            || section.title.len() > 120
            || section.item_ids.len() > 2000
        {
            return Err(bad("Invalid section."));
        }
        for id in &section.item_ids {
            if !known.contains(id) || !seen.insert(id) {
                return Err(bad("Layout contains an unknown or duplicate service."));
            }
        }
    }
    let value = serde_json::to_string(&layout.sections).map_err(anyhow::Error::from)?;
    conn.execute("INSERT INTO settings(key,value) VALUES('layout',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value])?;
    Ok(Json(json!({"ok":true})))
}

async fn get_templates(State(app): State<App>) -> ApiResult<Json<Value>> {
    let conn = app.db.lock().unwrap();
    let templates = crate::templates::load(&conn)?;
    let ctx = crate::templates::context(
        "Sacho",
        "https://status.example.com",
        &json!({"event":"incident.started"}),
    );
    Ok(Json(
        json!({"templates":templates,"defaults":crate::templates::Templates::default(),"variables":ctx.as_object().unwrap().keys().collect::<Vec<_>>()}),
    ))
}
async fn save_templates(
    State(app): State<App>,
    Json(templates): Json<crate::templates::Templates>,
) -> ApiResult<Json<Value>> {
    templates.validate().map_err(|e| bad(&e.to_string()))?;
    app.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES('notification_templates',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&templates).map_err(anyhow::Error::from)?])?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplatePreview {
    templates: crate::templates::Templates,
    event: String,
}
async fn preview_templates(Json(draft): Json<TemplatePreview>) -> ApiResult<Json<Value>> {
    if !matches!(
        draft.event.as_str(),
        "incident.started" | "incident.resolved" | "subscription.verify"
    ) {
        return Err(bad("Unknown preview event"));
    }
    draft
        .templates
        .validate()
        .map_err(|e| bad(&e.to_string()))?;
    let payload = json!({"event":draft.event,"name":"Example service","status":if draft.event=="incident.resolved" {"operational"} else {"outage"},"delivery_id":"example-delivery","incident_id":"example-incident","item_id":"example-item","token":"example","unsubscribe_token":"example","at":1700000000,"group":"Example","description":"Example description","display_value":"42.5","display_label":"TPS","metric_unit":" TPS","latency_ms":25});
    let ctx = crate::templates::context("Sacho", "https://status.example.com", &payload);
    let (subject, body) = draft
        .templates
        .email(&ctx)
        .map_err(|e| bad(&e.to_string()))?;
    let webhook = draft
        .templates
        .webhook(&ctx, &payload)
        .map_err(|e| bad(&e.to_string()))?;
    Ok(Json(
        json!({"subject":subject,"body":body,"webhook":webhook}),
    ))
}

async fn validate_draft(
    State(app): State<App>,
    Json(spec): Json<CheckSpec>,
) -> ApiResult<Json<Value>> {
    let mut report = crate::setup::validate(&spec, &app.cfg);
    if let Err(error) = validate_worker(&app.db.lock().unwrap(), &app.cfg, &spec) {
        report["valid"] = json!(false);
        report["errors"]
            .as_array_mut()
            .unwrap()
            .push(json!(error.1));
    }
    Ok(Json(report))
}
async fn examples() -> Json<Value> {
    Json(crate::setup::catalogue())
}
async fn export_settings(State(app): State<App>) -> ApiResult<Json<Value>> {
    Ok(Json(crate::setup::export(
        &app.db.lock().unwrap(),
        &app.cfg,
    )?))
}
async fn notification_policy(State(app): State<App>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(crate::monitoring::policy(
        &app.db.lock().unwrap()
    )?)))
}
async fn save_notification_policy(
    State(app): State<App>,
    Json(policy): Json<crate::monitoring::NotificationPolicy>,
) -> ApiResult<Json<Value>> {
    policy.validate().map_err(|e| bad(&e.to_string()))?;
    app.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES('notification_policy',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&policy).map_err(anyhow::Error::from)?])?;
    Ok(Json(json!({"ok":true})))
}

//! Per-monitor bearer reporting keys, shown once and stored only as hashes.
use crate::{
    db::{self, Item, Observation},
    server::{ApiError, App},
};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
fn key(id: &str) -> String {
    format!("push.token.{id}")
}
fn item(app: &App, id: &str) -> Result<Item> {
    db::items(&app.db.lock().unwrap())?
        .into_iter()
        .find(|i| i.id == id && i.spec.kind == "push")
        .ok_or_else(|| anyhow::anyhow!("push monitor not found"))
}
pub async fn issue(
    State(app): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    item(&app, &id)?;
    let token = format!(
        "sp_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    app.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key(&id),crate::enrollment::hash(&token)])?;
    Ok(Json(
        json!({"token":token,"endpoint":format!("{}/api/push/{id}",app.cfg.public_url.trim_end_matches('/')),"shown_once":true}),
    ))
}
pub async fn revoke(
    State(app): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    app.db
        .lock()
        .unwrap()
        .execute("DELETE FROM settings WHERE key=?", [key(&id)])?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub status: String,
    pub data: Value,
}
pub async fn report(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(report): Json<Report>,
) -> Result<Json<Value>, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|t| t.len() <= 256);
    let mut conn = app.db.lock().unwrap();
    let stored: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?", [key(&id)], |r| {
            r.get(0)
        })
        .optional()?;
    if token.is_none_or(|t| stored.as_deref() != Some(crate::enrollment::hash(t).as_str())) {
        return Err(ApiError::unauthorized());
    }
    let item = db::items(&conn)?
        .into_iter()
        .find(|i| i.id == id && i.spec.kind == "push" && i.spec.enabled)
        .ok_or_else(|| anyhow::anyhow!("active push monitor not found"))?;
    let obs = observation(&item, &report)?;
    db::record(&mut conn, &item, &obs)?;
    Ok(Json(json!({"ok":true,"recorded_at":db::now()})))
}
fn observation(item: &Item, report: &Report) -> Result<Observation> {
    ensure!(
        matches!(
            report.status.as_str(),
            "operational" | "degraded" | "outage"
        ),
        "invalid reported status"
    );
    let output = serde_json::to_string(&report.data)?;
    ensure!(output.len() <= 65536, "report exceeds output limit");
    let mut status = report.status.clone();
    let mut detail = String::new();
    if !item.spec.contains.is_empty() && !output.contains(&item.spec.contains) {
        status = "outage".into();
        detail = "reported text condition failed".into();
    }
    if let Some(threshold) = item.spec.threshold
        && !crate::monitoring::number(&output, &item.spec.json_pointer)
            .is_some_and(|n| crate::engine::compare(n, threshold, &item.spec.comparison))
    {
        if status != "outage" {
            status = item.spec.failure_status.clone();
        }
        detail = "reported numeric condition failed".into();
    }
    if let Some(rules) = &item.spec.value_rules {
        let severity = crate::monitoring::number(&output, &rules.pointer)
            .map(|n| rules.severity(n))
            .unwrap_or(&item.spec.failure_status);
        if severity == "outage" || (severity == "degraded" && status == "operational") {
            status = severity.into();
            detail = "reported value rule failed".into();
        }
    }
    Ok(Observation {
        status,
        latency_ms: 0,
        output,
        detail,
    })
}
pub fn expire(conn: &mut rusqlite::Connection, item: &Item) -> Result<()> {
    // One missing-report observation per interval; retain the last real scalar.
    db::record(
        conn,
        item,
        &Observation {
            status: item.spec.failure_status.clone(),
            latency_ms: 0,
            output: String::new(),
            detail: "expected push report was not received within the configured interval".into(),
        },
    )
}

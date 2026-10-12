//! Scalar severity, safe setup helpers and durable notification controls.
use crate::db::{self, CheckSpec};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueRules {
    pub pointer: String,
    pub direction: String,
    pub warning: Option<f64>,
    pub critical: Option<f64>,
}
impl ValueRules {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pointer.is_empty() || self.pointer.starts_with('/'),
            "value pointer must start with /"
        );
        ensure!(
            self.pointer.len() <= 1024 && matches!(self.direction.as_str(), "low" | "high"),
            "invalid value rules"
        );
        ensure!(
            self.warning.is_some() || self.critical.is_some(),
            "set a warning or critical threshold"
        );
        ensure!(
            self.warning.is_none_or(f64::is_finite) && self.critical.is_none_or(f64::is_finite),
            "thresholds must be finite"
        );
        if let (Some(w), Some(c)) = (self.warning, self.critical) {
            ensure!(
                if self.direction == "low" {
                    c <= w
                } else {
                    c >= w
                },
                "critical threshold must be more severe than warning"
            );
        }
        Ok(())
    }
    pub fn severity(&self, value: f64) -> &'static str {
        let crossed = |t: f64| {
            if self.direction == "low" {
                value <= t
            } else {
                value >= t
            }
        };
        if self.critical.is_some_and(crossed) {
            "outage"
        } else if self.warning.is_some_and(crossed) {
            "degraded"
        } else {
            "operational"
        }
    }
}
pub fn number(output: &str, pointer: &str) -> Option<f64> {
    let value: Value = serde_json::from_str(output).ok()?;
    let value = if pointer.is_empty() {
        &value
    } else {
        value.pointer(pointer)?
    };
    value.as_f64().filter(|v| v.is_finite())
}
pub fn metric_pointer(spec: &CheckSpec) -> &str {
    if spec.publish_trend {
        if !spec.trend_pointer.is_empty() {
            return &spec.trend_pointer;
        }
        if let Some(r) = &spec.value_rules {
            return &r.pointer;
        }
        if !spec.display_pointer.is_empty() {
            return &spec.display_pointer;
        }
    }
    &spec.json_pointer
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationPolicy {
    pub settle_seconds: u64,
    pub cooldown_seconds: u64,
    pub maintenance_until: i64,
}
impl NotificationPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.settle_seconds <= 3600 && self.cooldown_seconds <= 86400,
            "notification delay/cooldown exceeds limit"
        );
        ensure!(
            self.maintenance_until >= 0 && self.maintenance_until <= db::now() + 30 * 86400,
            "maintenance must end within 30 days"
        );
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.settle_seconds > 0 || self.cooldown_seconds > 0 || self.maintenance_until > 0
    }
}
pub fn policy(conn: &Connection) -> Result<NotificationPolicy> {
    let text: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='notification_policy'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(text
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}
pub fn queue_monitor(conn: &Connection, kind: &str, to: &str, event: &Value) -> Result<()> {
    let policy = policy(conn)?;
    if event["event"] == "incident.resolved" {
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE kind=? AND destination=? AND json_extract(payload,'$.incident_id')=? AND json_extract(payload,'$.event')='incident.started' AND COALESCE(error,'') NOT LIKE 'suppressed:%')",params![kind,to,event["incident_id"].as_str()],|r|r.get(0))?;
        if !exists {
            return Ok(());
        }
    }
    db::queue(conn, kind, to, event)?;
    if policy.maintenance_until > db::now() {
        conn.execute("UPDATE outbox SET attempts=8,error='suppressed: maintenance' WHERE id=last_insert_rowid()",[])?;
        return Ok(());
    }
    if event["event"] == "incident.started" {
        let last:Option<i64>=conn.query_row("SELECT MAX(delivered_at) FROM outbox WHERE kind=? AND destination=? AND json_extract(payload,'$.item_id')=? AND json_extract(payload,'$.event')='incident.started'",params![kind,to,event["item_id"].as_str()],|r|r.get(0))?;
        let next = (db::now() + policy.settle_seconds as i64)
            .max(last.unwrap_or(0) + policy.cooldown_seconds as i64);
        conn.execute(
            "UPDATE outbox SET next_at=? WHERE id=last_insert_rowid()",
            [next],
        )?;
    }
    Ok(())
}
/// Called immediately before a send. Verification mail is never muted.
pub fn permit_delivery(
    conn: &Connection,
    id: i64,
    kind: &str,
    to: &str,
    event: &Value,
) -> Result<bool> {
    let Some(name) = event["event"]
        .as_str()
        .filter(|e| e.starts_with("incident."))
    else {
        return Ok(true);
    };
    let policy = policy(conn)?;
    let reason = if policy.maintenance_until > db::now() {
        Some("maintenance")
    } else if policy.enabled() && name == "incident.started" {
        let open: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM incidents WHERE id=? AND resolved_at IS NULL)",
            [event["incident_id"].as_str()],
            |r| r.get(0),
        )?;
        if open {
            None
        } else {
            Some("recovered before delivery")
        }
    } else if policy.enabled() && name == "incident.resolved" {
        let sent:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE kind=? AND destination=? AND delivered_at IS NOT NULL AND json_extract(payload,'$.incident_id')=? AND json_extract(payload,'$.event')='incident.started')",params![kind,to,event["incident_id"].as_str()],|r|r.get(0))?;
        if sent {
            None
        } else {
            Some("no delivered problem notification")
        }
    } else {
        None
    };
    if let Some(reason) = reason {
        conn.execute(
            "UPDATE outbox SET attempts=8,error=? WHERE id=? AND delivered_at IS NULL",
            params![format!("suppressed: {reason}"), id],
        )?;
        return Ok(false);
    }
    Ok(true)
}
pub fn heartbeat(
    conn: &Connection,
    node: &str,
    version: Option<&str>,
    platform: Option<&str>,
) -> Result<()> {
    let key = format!("worker.heartbeat.{node}");
    let last: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?", [&key], |r| {
            r.get(0)
        })
        .optional()?;
    if last
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["at"].as_i64().or(v.as_i64()))
        .is_none_or(|at| db::now() - at >= 10)
    {
        conn.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,json!({"at":db::now(),"version":version.filter(|v|v.len()<=64 && v.bytes().all(|b|b.is_ascii_alphanumeric() || b".-_".contains(&b))),"platform":platform.filter(|v|v.len()<=64 && v.bytes().all(|b|b.is_ascii_alphanumeric() || b".-_".contains(&b)))}).to_string()])?;
    }
    Ok(())
}
pub fn worker_health(conn: &Connection, node: &str, revoked: bool) -> Result<Value> {
    let last: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("worker.heartbeat.{node}")],
            |r| r.get(0),
        )
        .optional()?;
    let report = last
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_default();
    let last = report["at"].as_i64().or(report.as_i64());
    let mut stmt=conn.prepare("SELECT r.item_id,i.spec,r.lease_until FROM remote_runs r JOIN items i ON i.id=r.item_id WHERE r.worker=? AND r.result_hash IS NULL AND r.lease_until>? AND r.revision=i.revision AND json_extract(i.spec,'$.enabled')=1 AND json_extract(i.spec,'$.worker')=?")?;
    let jobs=stmt.query_map(params![node,db::now(),node],|r|{let text:String=r.get(1)?;let spec:Value=serde_json::from_str(&text).unwrap_or_default();Ok(json!({"id":r.get::<_,String>(0)?,"name":spec["name"],"lease_until":r.get::<_,i64>(2)?}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(
        json!({"reported_version":report["version"],"reported_platform":report["platform"],"last_seen":last,"connection":if revoked{"revoked"}else if last.is_none(){"unknown"}else if last.is_some_and(|at|db::now()-at<=30){"online"}else{"offline"},"active_checks":jobs}),
    )
}

/// Delayed notifications describe the latest approved scalar and current severity.
pub fn refresh_event(conn: &Connection, event: &mut Value) -> Result<()> {
    if event["event"] != "incident.started" {
        return Ok(());
    }
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM incidents WHERE id=? AND resolved_at IS NULL",
            [event["incident_id"].as_str()],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(status) = status {
        event["status"] = json!(status);
    }
    let encoded: Option<String> = conn
        .query_row(
            "SELECT spec FROM items WHERE id=?",
            [event["item_id"].as_str()],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(encoded) = encoded {
        let spec: CheckSpec = serde_json::from_str(&encoded)?;
        let value = if spec.publish_value {
            conn.query_row("SELECT display_value FROM checks WHERE item_id=? AND display_value IS NOT NULL ORDER BY id DESC LIMIT 1",[event["item_id"].as_str()],|r|r.get::<_,String>(0)).optional()?
        } else {
            None
        };
        event["display_value"] = json!(value);
        event["display_label"] = json!(spec.display_label);
        event["metric_unit"] = json!(spec.metric_unit);
    }
    Ok(())
}

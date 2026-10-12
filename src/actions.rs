//! Per-check opt-in actions, kept separate from monitoring and notification delivery.
use crate::{
    config::Config,
    db::{self, CheckSpec, Db, Item, Observation},
};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Action {
    pub trigger: String,
    pub mode: String,
    pub kind: String,
    pub target: String,
    pub body: String,
    pub headers_env: std::collections::BTreeMap<String, String>,
    pub after_count: u32,
    pub delay_seconds: u64,
    pub cooldown_seconds: u64,
    pub max_attempts: u32,
    pub retry_seconds: u64,
    pub wait_after_seconds: u64,
    pub timeout_seconds: u64,
}
impl Default for Action {
    fn default() -> Self {
        Self {
            trigger: "failure".into(),
            mode: "transition".into(),
            kind: "https".into(),
            target: String::new(),
            body: "{}".into(),
            headers_env: Default::default(),
            after_count: 1,
            delay_seconds: 0,
            cooldown_seconds: 300,
            max_attempts: 1,
            retry_seconds: 30,
            wait_after_seconds: 0,
            timeout_seconds: 30,
        }
    }
}
impl Action {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.trigger.as_str(), "success" | "failure")
                && matches!(self.mode.as_str(), "transition" | "repeat"),
            "invalid action trigger/mode"
        );
        ensure!(
            matches!(self.kind.as_str(), "shell" | "python" | "lua" | "https"),
            "invalid action kind"
        );
        ensure!(
            !self.target.trim().is_empty() && self.target.len() <= 16384 && self.body.len() <= 8192,
            "action target/body exceeds limit"
        );
        ensure!(
            (1..=100).contains(&self.after_count)
                && (1..=5).contains(&self.max_attempts)
                && (1..=300).contains(&self.timeout_seconds),
            "invalid action counts/timeout"
        );
        ensure!(
            self.delay_seconds <= 86400
                && self.cooldown_seconds <= 86400
                && self.wait_after_seconds <= 86400
                && (1..=86400).contains(&self.retry_seconds),
            "invalid action timing"
        );
        ensure!(
            self.mode != "repeat" || self.cooldown_seconds >= 5,
            "repeating actions require cooldown of at least five seconds"
        );
        if self.kind == "lua" {
            crate::engine::validate_lua(&self.target)?;
        }
        if self.kind == "https" {
            let url: reqwest::Url = self.target.parse()?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "action URL must be HTTP(S) without embedded credentials"
            );
            let mut env = minijinja::Environment::new();
            env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
            env.set_fuel(Some(10000));
            env.add_template("body", &self.body)?;
        }
        ensure!(self.headers_env.len() <= 16, "too many action headers");
        for (header, name) in &self.headers_env {
            reqwest::header::HeaderName::from_bytes(header.as_bytes())?;
            ensure!(
                crate::secrets::valid_name(name),
                "invalid action header input"
            );
        }
        Ok(())
    }
}
pub fn queue(
    conn: &Connection,
    item: &Item,
    obs: &Observation,
    old: &str,
    next: &str,
    failures: u32,
    successes: u32,
) -> Result<()> {
    let bad = matches!(next, "outage" | "degraded");
    for action in &item.spec.actions {
        let failure = action.trigger == "failure";
        let count = if failure { failures } else { successes };
        let threshold = action.after_count.max(if failure {
            item.spec.failure_threshold
        } else {
            item.spec.recovery_threshold
        });
        if failure != bad
            || (failure && obs.status == "operational")
            || (!failure && obs.status != "operational")
            || count < threshold
        {
            continue;
        }
        if action.mode == "transition" && count != threshold {
            continue;
        }
        let destination = format!("{}:{}", item.id, action.trigger);
        let last: Option<i64> = conn.query_row(
            "SELECT MAX(created_at) FROM outbox WHERE kind='action' AND destination=?",
            [&destination],
            |r| r.get(0),
        )?;
        let pending:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE kind='action' AND destination=? AND delivered_at IS NULL AND attempts < CAST(json_extract(payload,'$.action.max_attempts') AS INTEGER))",[&destination],|r|r.get(0))?;
        if pending || last.is_some_and(|at| db::now() - at < action.cooldown_seconds as i64) {
            continue;
        }
        let context = json!({"event":if failure{"check.failed"}else{"check.succeeded"},"item_id":item.id,"name":item.spec.name,"status":next,"previous_status":old,"at":db::now(),"latency_ms":obs.latency_ms,"display_value":db::display_value(&item.spec,&obs.output)});
        let payload = json!({"action":action,"context":context,"revision":item.revision,"item_id":item.id,"delivery_id":uuid::Uuid::new_v4().to_string()});
        conn.execute("INSERT INTO outbox(kind,destination,payload,next_at,created_at) VALUES('action',?,?,?,?)",params![destination,payload.to_string(),db::now()+action.delay_seconds as i64,db::now()])?;
    }
    Ok(())
}
async fn execute(cfg: &Config, action: &Action, context: &Value, delivery_id: &str) -> Result<()> {
    if action.kind == "https" {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.set_fuel(Some(10000));
        let rendered = env.render_str(&action.body, context)?;
        ensure!(rendered.len() <= 65536, "action body exceeds limit");
        let body: Value = serde_json::from_str(&rendered)?;
        ensure!(body.is_object(), "action body must be JSON object");
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(action.timeout_seconds))
            .build()?;
        let mut req = client
            .post(&action.target)
            .header("Idempotency-Key", delivery_id)
            .json(&body);
        for (header, name) in &action.headers_env {
            ensure!(
                cfg.probe_names().contains(name),
                "action input is not allowed"
            );
            req = req.header(header, cfg.environment(name)?);
        }
        let response = req.send().await?;
        ensure!(response.status().is_success(), "action endpoint failed");
    } else {
        let mut cfg = cfg.clone();
        cfg.probe_env.push("SACHO_EVENT_JSON".into());
        cfg.probe_overrides
            .insert("SACHO_EVENT_JSON".into(), context.to_string());
        let spec = CheckSpec {
            name: "Action".into(),
            kind: action.kind.clone(),
            target: action.target.clone(),
            interval_seconds: action.timeout_seconds.max(5),
            timeout_seconds: action.timeout_seconds,
            ..Default::default()
        };
        let item = Item {
            id: "action".into(),
            spec,
            status: "unknown".into(),
            last_check: None,
            latency_ms: None,
            failures: 0,
            successes: 0,
            revision: 1,
        };
        let result = crate::engine::probe_with_config(&item, &cfg).await;
        ensure!(result.status == "operational", "action execution failed");
    }
    Ok(())
}
pub async fn worker(db: Db, cfg: Arc<Config>, mut stop: watch::Receiver<bool>) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {_=stop.changed()=>break,_=tick.tick()=>{}}
        let job = (|| -> Result<Option<(i64, String, u32)>> {
            let conn = db.lock().unwrap();
            Ok(conn.query_row("SELECT id,payload,attempts FROM outbox WHERE kind='action' AND delivered_at IS NULL AND attempts < CAST(json_extract(payload,'$.action.max_attempts') AS INTEGER) AND next_at<=? ORDER BY id LIMIT 1",[db::now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
        })();
        let Ok(Some((id, text, attempts))) = job else {
            continue;
        };
        let Ok(payload) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Ok(action) = serde_json::from_value::<Action>(payload["action"].clone()) else {
            continue;
        };
        let item_id = payload["item_id"].as_str().unwrap_or_default();
        let valid = {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT spec,status,revision FROM items WHERE id=?",
                [item_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u64>(2)?,
                    ))
                },
            )
            .optional()
        };
        let permitted = valid
            .ok()
            .flatten()
            .is_some_and(|(spec, status, revision)| {
                serde_json::from_str::<CheckSpec>(&spec).is_ok_and(|s| s.enabled)
                    && Some(revision) == payload["revision"].as_u64()
                    && (action.trigger == "failure")
                        == matches!(status.as_str(), "outage" | "degraded")
            });
        if !permitted {
            let _=db.lock().unwrap().execute("UPDATE outbox SET attempts=CAST(json_extract(payload,'$.action.max_attempts') AS INTEGER),error='suppressed: action state or configuration changed' WHERE id=?",[id]);
            continue;
        }
        // Persist attempt before execution. Explicit retries may run again after a crash;
        // targets must be idempotent. Default max_attempts=1 never retries an uncertain attempt.
        if db.lock().unwrap().execute("UPDATE outbox SET attempts=attempts+1,next_at=?,error='Action running or interrupted' WHERE id=?",params![db::now()+action.timeout_seconds as i64+action.retry_seconds as i64,id]).is_err(){continue;}
        let mut context = payload["context"].clone();
        context["attempt"] = json!(attempts + 1);
        context["delivery_id"] = payload["delivery_id"].clone();
        let result = tokio::select! {_=stop.changed()=>return,r=execute(&cfg,&action,&context,payload["delivery_id"].as_str().unwrap_or_default())=>r};
        let conn = db.lock().unwrap();
        if result.is_ok() {
            let _ = conn.execute(
                "UPDATE outbox SET delivered_at=?,error=NULL WHERE id=?",
                params![db::now(), id],
            );
        } else {
            let _=conn.execute("UPDATE outbox SET next_at=?,error='Action failed; inspect target, inputs and execution permissions' WHERE id=?",params![db::now()+action.retry_seconds as i64,id]);
        }
        if action.wait_after_seconds > 0 {
            let _=conn.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("schedule.hold.{item_id}"),(db::now()+action.wait_after_seconds as i64).to_string()]);
        }
    }
}

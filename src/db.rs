use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub type Db = Arc<Mutex<Connection>>;
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CheckSpec {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub worker: String,
    pub description: String,
    pub group: String,
    pub kind: String,
    pub target: String,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cron: String,
    #[serde(skip_serializing_if = "default_timezone")]
    pub schedule_timezone: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub initial_delay_seconds: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prometheus_query: String,
    #[serde(skip_serializing_if = "default_dns_type")]
    pub dns_record_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dns_server: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dns_expected: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<crate::actions::Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping: Option<crate::network_probes::Ping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snmp: Option<crate::network_probes::Snmp>,
    pub expected_status: u16,
    pub headers_env: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_env: Vec<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub input_bindings: std::collections::BTreeMap<String, crate::scoped_inputs::Binding>,
    #[serde(skip_serializing)]
    pub input_updates: Vec<crate::scoped_inputs::Update>,
    pub expected_exit: i32,
    pub contains: String,
    pub json_pointer: String,
    pub comparison: String,
    pub threshold: Option<f64>,
    pub degraded_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_rules: Option<crate::monitoring::ValueRules>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub publish_trend: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub trend_pointer: String,
    pub failure_threshold: u32,
    #[serde(skip_serializing_if = "default_failure_status")]
    pub failure_status: String,
    pub recovery_threshold: u32,
    pub enabled: bool,
    pub publish_value: bool,
    pub metric_unit: String,
    pub display_pointer: String,
    pub display_label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub public_message_pointer: String,
}
fn default_timezone(v: &str) -> bool {
    v == "UTC"
}
fn default_dns_type(v: &str) -> bool {
    v == "A"
}
fn is_zero(v: &u64) -> bool {
    *v == 0
}
fn default_failure_status(value: &str) -> bool {
    value == "outage"
}
impl Default for CheckSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            worker: String::new(),
            description: String::new(),
            group: "Services".into(),
            kind: "http".into(),
            target: String::new(),
            interval_seconds: 60,
            timeout_seconds: 10,
            cron: String::new(),
            schedule_timezone: "UTC".into(),
            initial_delay_seconds: 0,
            prometheus_query: String::new(),
            dns_record_type: "A".into(),
            dns_server: String::new(),
            dns_expected: vec![],
            actions: vec![],
            ping: None,
            snmp: None,
            expected_status: 200,
            headers_env: Default::default(),
            required_env: Vec::new(),
            input_bindings: Default::default(),
            input_updates: vec![],
            expected_exit: 0,
            contains: String::new(),
            json_pointer: String::new(),
            comparison: "lt".into(),
            threshold: None,
            degraded_ms: None,
            value_rules: None,
            publish_trend: false,
            trend_pointer: String::new(),
            failure_threshold: 1,
            failure_status: "outage".into(),
            recovery_threshold: 1,
            enabled: true,
            publish_value: false,
            metric_unit: String::new(),
            display_pointer: String::new(),
            display_label: String::new(),
            public_message_pointer: String::new(),
        }
    }
}
impl CheckSpec {
    pub fn validate(&self) -> Result<()> {
        crate::network_probes::validate(self)?;
        crate::scoped_inputs::validate(self)?;
        crate::scheduling::validate(self)?;
        crate::sources::validate(self)?;
        ensure!(self.actions.len() <= 2, "at most two actions per monitor");
        ensure!(
            self.actions.len() < 2 || self.actions[0].trigger != self.actions[1].trigger,
            "only one action per trigger"
        );
        for action in &self.actions {
            action.validate()?;
        }
        if let Some(rules) = &self.value_rules {
            rules.validate()?;
        }
        ensure!(
            self.trend_pointer.len() <= 1024
                && (self.trend_pointer.is_empty() || self.trend_pointer.starts_with('/')),
            "invalid trend JSON pointer"
        );
        ensure!(
            matches!(self.failure_status.as_str(), "outage" | "degraded"),
            "failure status must be outage or degraded"
        );
        ensure!(
            self.worker.is_empty() || crate::remote::valid_node(&self.worker),
            "invalid worker ID"
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 120,
            "name required, up to 120 bytes"
        );
        ensure!(
            self.public_message_pointer.len() <= 1024
                && self.display_label.len() <= 120
                && self.display_pointer.len() <= 1024
                && self.metric_unit.len() <= 20
                && self.description.len() <= 2000
                && self.group.len() <= 120
                && self.target.len() <= 65536
                && self.contains.len() <= 4096
                && self.json_pointer.len() <= 1024,
            "field too long"
        );
        ensure!(!self.target.trim().is_empty(), "target or script required");
        ensure!(
            matches!(
                self.kind.as_str(),
                "http"
                    | "shell"
                    | "python"
                    | "lua"
                    | "prometheus"
                    | "dns"
                    | "tcp"
                    | "ping"
                    | "snmp"
                    | "push"
            ),
            "unsupported check kind"
        );
        ensure!(
            (5..=86400).contains(&self.interval_seconds),
            "interval must be 5..86400 seconds"
        );
        ensure!(
            (1..=300).contains(&self.timeout_seconds)
                && self.timeout_seconds <= self.interval_seconds,
            "timeout must be 1..300 and no more than interval"
        );
        ensure!(
            (1..=100).contains(&self.failure_threshold)
                && (1..=100).contains(&self.recovery_threshold),
            "consecutive thresholds must be 1..100"
        );
        ensure!(
            matches!(self.comparison.as_str(), "lt" | "lte" | "gt" | "gte" | "eq"),
            "comparison must be lt/lte/gt/gte/eq"
        );
        ensure!(
            (100..=599).contains(&self.expected_status),
            "invalid expected HTTP status"
        );
        ensure!(
            self.required_env.len() <= 64
                && self.required_env.iter().all(|n| !n.is_empty()
                    && n.len() <= 128
                    && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')),
            "invalid required environment names"
        );
        ensure!(self.headers_env.len() <= 16, "too many HTTP headers");
        for (header, env) in &self.headers_env {
            reqwest::header::HeaderName::from_bytes(header.as_bytes())?;
            ensure!(
                !env.is_empty()
                    && env.len() <= 128
                    && env.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "header value must name an allowed environment variable"
            );
        }
        if self.kind == "http" || self.kind == "prometheus" {
            let url = reqwest::Url::parse(&self.target)?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.host_str().is_some(),
                "target must be HTTP(S) without embedded credentials"
            );
        }
        ensure!(
            self.display_pointer.is_empty() || self.display_pointer.starts_with('/'),
            "display JSON pointer must start with /"
        );
        ensure!(
            self.public_message_pointer.is_empty() || self.public_message_pointer.starts_with('/'),
            "public message JSON pointer must start with /"
        );
        ensure!(
            self.json_pointer.is_empty() || self.json_pointer.starts_with('/'),
            "JSON pointer must start with /"
        );
        ensure!(
            self.threshold.is_none_or(|n| n.is_finite()),
            "threshold must be finite"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub id: String,
    pub spec: CheckSpec,
    pub status: String,
    pub last_check: Option<i64>,
    pub latency_ms: Option<i64>,
    pub failures: u32,
    pub successes: u32,
    pub revision: u64,
}
pub fn open(path: &str) -> Result<Db> {
    let conn = Connection::open(path)?;
    let schema: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        schema <= 1,
        "database schema is newer than this binary; upgrade Sacho"
    );
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
    CREATE TABLE IF NOT EXISTS items(id TEXT PRIMARY KEY, spec TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'unknown', last_check INTEGER, latency_ms INTEGER, failures INTEGER NOT NULL DEFAULT 0, successes INTEGER NOT NULL DEFAULT 0, revision INTEGER NOT NULL DEFAULT 1);
    CREATE TABLE IF NOT EXISTS checks(id INTEGER PRIMARY KEY, item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE, at INTEGER NOT NULL, status TEXT NOT NULL, latency_ms INTEGER NOT NULL, output TEXT NOT NULL, detail TEXT NOT NULL, metric_value REAL, display_value TEXT);
    CREATE INDEX IF NOT EXISTS checks_time ON checks(item_id,at);
    CREATE INDEX IF NOT EXISTS checks_latest ON checks(item_id,id DESC);
    CREATE INDEX IF NOT EXISTS checks_healthy_time ON checks(item_id,at DESC) WHERE status='operational';
    CREATE INDEX IF NOT EXISTS checks_display_latest ON checks(item_id,id DESC) WHERE display_value IS NOT NULL AND display_value != '';
    CREATE TABLE IF NOT EXISTS incidents(id TEXT PRIMARY KEY, item_id TEXT REFERENCES items(id) ON DELETE SET NULL, name TEXT NOT NULL, status TEXT NOT NULL, opened_at INTEGER NOT NULL, resolved_at INTEGER, detail TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS incidents_time ON incidents(opened_at);
    CREATE TABLE IF NOT EXISTS subscribers(id TEXT PRIMARY KEY, email TEXT UNIQUE NOT NULL, token TEXT UNIQUE NOT NULL, verified INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS webhooks(id TEXT PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1);
    CREATE TABLE IF NOT EXISTS outbox(id INTEGER PRIMARY KEY, kind TEXT NOT NULL, destination TEXT NOT NULL, payload TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL, delivered_at INTEGER, error TEXT, created_at INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS outbox_due ON outbox(next_at) WHERE delivered_at IS NULL;
    CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS remote_runs(item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE, run_id TEXT NOT NULL UNIQUE, worker TEXT NOT NULL, revision INTEGER NOT NULL, lease_until INTEGER NOT NULL, result_hash TEXT);
    PRAGMA user_version=1;")?;
    Ok(Arc::new(Mutex::new(conn)))
}
pub fn items(conn: &Connection) -> Result<Vec<Item>> {
    let mut stmt = conn.prepare("SELECT id,spec,status,last_check,latency_ms,failures,successes,revision FROM items ORDER BY json_extract(spec,'$.group'),json_extract(spec,'$.name')")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, u32>(5)?,
            r.get::<_, u32>(6)?,
            r.get::<_, u64>(7)?,
        ))
    })?;
    let mut result = vec![];
    for row in rows {
        let (id, spec, status, last_check, latency_ms, failures, successes, revision) = row?;
        result.push(Item {
            id,
            spec: serde_json::from_str(&spec)?,
            status,
            last_check,
            latency_ms,
            failures,
            successes,
            revision,
        });
    }
    Ok(result)
}
pub fn save(conn: &mut Connection, id: Option<String>, spec: &CheckSpec) -> Result<String> {
    spec.validate()?;
    let encoded = serde_json::to_string(spec)?;
    if let Some(id) = id {
        let previous: String =
            conn.query_row("SELECT spec FROM items WHERE id=?", [&id], |r| r.get(0))?;
        let previous: CheckSpec = serde_json::from_str(&previous)?;
        let tx = conn.transaction()?;
        if crate::monitoring::metric_pointer(&previous) != crate::monitoring::metric_pointer(spec) {
            tx.execute("UPDATE checks SET metric_value=NULL WHERE item_id=?", [&id])?;
        }
        ensure!(
            tx.execute(
                "UPDATE items SET spec=?,revision=revision+1,failures=0,successes=0 WHERE id=?",
                params![encoded, id]
            )? == 1,
            "item not found"
        );
        tx.commit()?;
        Ok(id)
    } else {
        let id = Uuid::new_v4().to_string();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO items(id,spec) VALUES(?,?)",
            params![id, encoded],
        )?;
        tx.execute(
            "INSERT INTO settings(key,value) VALUES(?,?)",
            params![format!("schedule.start.{id}"), now().to_string()],
        )?;
        tx.commit()?;
        Ok(id)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub status: String,
    pub latency_ms: i64,
    pub output: String,
    pub detail: String,
}
// Observations, state transitions, incidents and notification jobs commit together.
pub fn record(conn: &mut Connection, item: &Item, obs: &Observation) -> Result<()> {
    let tx = conn.transaction()?;
    record_transaction(&tx, item, obs)?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn record_transaction(
    tx: &rusqlite::Transaction<'_>,
    item: &Item,
    obs: &Observation,
) -> Result<()> {
    let current: Option<(String, u32, u32, u64)> = tx
        .query_row(
            "SELECT status,failures,successes,revision FROM items WHERE id=?",
            [&item.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((old, failures, successes, revision)) = current else {
        return Ok(());
    };
    if revision != item.revision {
        return Ok(());
    }
    let bad = obs.status != "operational";
    let failures = if bad { failures.saturating_add(1) } else { 0 };
    let successes = if bad { 0 } else { successes.saturating_add(1) };
    let next = if (bad && failures >= item.spec.failure_threshold)
        || (!bad && successes >= item.spec.recovery_threshold)
    {
        obs.status.as_str()
    } else {
        old.as_str()
    };
    tx.execute(
        "DELETE FROM settings WHERE key=?",
        [format!("schedule.run.{}", item.id)],
    )?;
    let at = now();
    let metric =
        crate::monitoring::number(&obs.output, crate::monitoring::metric_pointer(&item.spec));
    let display = display_value(&item.spec, &obs.output);
    tx.execute("INSERT INTO checks(item_id,at,status,latency_ms,output,detail,metric_value,display_value) VALUES(?,?,?,?,?,?,?,?)",params![item.id,at,obs.status,obs.latency_ms,obs.output,obs.detail,metric,display])?;
    tx.execute(
        "UPDATE items SET status=?,last_check=?,latency_ms=?,failures=?,successes=? WHERE id=?",
        params![next, at, obs.latency_ms, failures, successes, item.id],
    )?;
    crate::actions::queue(tx, item, obs, &old, next, failures, successes)?;
    let old_bad = old == "outage" || old == "degraded";
    let next_bad = next == "outage" || next == "degraded";
    if !old_bad && next_bad {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO incidents(id,item_id,name,status,opened_at,detail) VALUES(?,?,?,?,?,?)",
            params![id, item.id, item.spec.name, next, at, obs.detail],
        )?;
        enqueue_event(
            tx,
            json!({"event":"incident.started","incident_id":id,"item_id":item.id,"name":item.spec.name,"status":next,"at":at,"group":item.spec.group,"description":item.spec.description,"display_value":display,"display_label":item.spec.display_label,"metric_unit":item.spec.metric_unit,"latency_ms":obs.latency_ms}),
        )?;
    } else if old_bad && !next_bad {
        let incident: Option<String> = tx
            .query_row(
                "SELECT id FROM incidents WHERE item_id=? AND resolved_at IS NULL LIMIT 1",
                [&item.id],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute("UPDATE incidents SET status='resolved',resolved_at=? WHERE item_id=? AND resolved_at IS NULL",params![at,item.id])?;
        enqueue_event(
            tx,
            json!({"event":"incident.resolved","incident_id":incident,"item_id":item.id,"name":item.spec.name,"status":next,"at":at,"group":item.spec.group,"description":item.spec.description,"display_value":display,"display_label":item.spec.display_label,"metric_unit":item.spec.metric_unit,"latency_ms":obs.latency_ms}),
        )?;
    } else if next_bad {
        tx.execute(
            "UPDATE incidents SET status=? WHERE item_id=? AND resolved_at IS NULL",
            params![next, item.id],
        )?;
    }
    Ok(())
}
pub fn queue(conn: &Connection, kind: &str, destination: &str, payload: &Value) -> Result<()> {
    let mut payload = payload.clone();
    ensure!(
        payload.is_object(),
        "notification payload must be an object"
    );
    payload["delivery_id"] = json!(Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO outbox(kind,destination,payload,next_at,created_at) VALUES(?,?,?,?,?)",
        params![
            kind,
            destination,
            serde_json::to_string(&payload)?,
            now(),
            now()
        ],
    )?;
    Ok(())
}
fn enqueue_event(conn: &Connection, event: Value) -> Result<()> {
    let mut stmt = conn.prepare("SELECT email,token FROM subscribers WHERE verified=1")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (email, token) = row?;
        let mut payload = event.clone();
        payload["unsubscribe_token"] = json!(token);
        crate::monitoring::queue_monitor(conn, "email", &email, &payload)?;
    }
    let mut stmt = conn.prepare("SELECT url FROM webhooks WHERE enabled=1")?;
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        crate::monitoring::queue_monitor(conn, "webhook", &row?, &event)?;
    }
    Ok(())
}
pub fn incidents(conn: &Connection, limit: u32) -> Result<Vec<Value>> {
    let mut stmt=conn.prepare("SELECT id,item_id,name,status,opened_at,resolved_at,detail FROM incidents ORDER BY opened_at DESC LIMIT ?")?;
    let rows=stmt.query_map([limit],|r|Ok(json!({"id":r.get::<_,String>(0)?,"item_id":r.get::<_,Option<String>>(1)?,"name":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"opened_at":r.get::<_,i64>(4)?,"resolved_at":r.get::<_,Option<i64>>(5)?,"detail":r.get::<_,String>(6)?})))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}
pub fn site(conn: &Connection, cfg: &crate::config::Config) -> Result<Value> {
    let get = |key: &str, default: &str| -> Result<String> {
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(default.into()))
    };
    Ok(
        json!({"title":get("title",&cfg.title)?,"description":get("description",&cfg.description)?,"layout":serde_json::from_str::<Value>(&get("layout","[]")?)?}),
    )
}

/// Display extraction never determines health. Only scalar text/number/bool is public.
pub fn display_value(spec: &CheckSpec, output: &str) -> Option<String> {
    if !spec.publish_value || output.trim().is_empty() {
        return None;
    }
    let parsed = serde_json::from_str::<Value>(output);
    let value = match parsed {
        Ok(value) => {
            let pointer = if spec.display_pointer.is_empty() {
                &spec.json_pointer
            } else {
                &spec.display_pointer
            };
            let value = if pointer.is_empty() {
                &value
            } else {
                value.pointer(pointer)?
            };
            match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return None,
            }
        }
        Err(_) => {
            if !spec.display_pointer.is_empty() {
                return None;
            }
            output.trim().into()
        }
    };
    Some(
        value
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect(),
    )
}

/// Only an explicitly selected JSON string is public; never expose raw errors.
pub fn public_message(spec: &CheckSpec, output: &str) -> Option<String> {
    if spec.public_message_pointer.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(output).ok()?;
    let text = value.pointer(&spec.public_message_pointer)?.as_str()?;
    Some(text.chars().filter(|c| !c.is_control()).take(240).collect())
}

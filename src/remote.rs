//! Single-controller, outbound-only remote monitoring workers.
use crate::{
    config::{Config, RemoteWorker, Secrets},
    db::{self, CheckSpec, Item, Observation},
};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path as FilePath, time::Duration};
use uuid::Uuid;

pub fn valid_node(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub protocol: u32,
    pub run_id: String,
    pub item_id: String,
    pub revision: u64,
    pub spec: CheckSpec,
    pub lease_until: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub protocol: u32,
    pub run_id: String,
    pub item_id: String,
    pub observation: Observation,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    protocol: u32,
    allow_scripts: bool,
}

type RemoteResult<T> = std::result::Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;
fn error(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"error": message})))
}
fn authorize<'a>(
    cfg: &'a Config,
    node: &str,
    headers: &HeaderMap,
) -> std::result::Result<&'a RemoteWorker, (StatusCode, Json<serde_json::Value>)> {
    let worker = cfg
        .remote_workers
        .iter()
        .find(|w| w.id == node)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Worker authentication required"))?;
    let supplied = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let expected = cfg
        .environment(&worker.token_env)
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "Worker authentication required"))?;
    if supplied.len() > 512
        || Sha256::digest(supplied.as_bytes()) != Sha256::digest(expected.as_bytes())
    {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "Worker authentication required",
        ));
    }
    Ok(worker)
}

pub fn claim(
    conn: &mut rusqlite::Connection,
    node: &str,
    allow_scripts: bool,
) -> Result<Option<Assignment>> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let active: i64 = tx.query_row(
        "SELECT COUNT(*) FROM remote_runs r JOIN items i ON i.id=r.item_id WHERE r.worker=? AND r.result_hash IS NULL AND r.lease_until>? AND r.revision=i.revision AND json_extract(i.spec,'$.worker')=? AND json_extract(i.spec,'$.enabled')=1",
        params![node, db::now(), node], |row| row.get(0),
    )?;
    if active > 0 {
        tx.commit()?;
        return Ok(None);
    }
    let mut items = db::items(&tx)?;
    // Prefer never-checked/oldest monitors so slow probes cannot starve later
    // names when a node has more work than it can complete per interval.
    items.sort_by(|a, b| a.last_check.cmp(&b.last_check).then(a.id.cmp(&b.id)));
    for item in items {
        if !item.spec.enabled
            || item.spec.worker != node
            || (!allow_scripts && item.spec.kind != "http")
        {
            continue;
        }
        let prior: Option<(u64, i64, Option<String>)> = tx
            .query_row(
                "SELECT revision,lease_until,result_hash FROM remote_runs WHERE item_id=?",
                [&item.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if prior.as_ref().is_some_and(|(revision, until, done)| {
            *revision == item.revision && done.is_none() && *until > db::now()
        }) {
            continue;
        }
        if item
            .last_check
            .is_some_and(|at| db::now() - at < item.spec.interval_seconds as i64)
        {
            continue;
        }
        let run_id = Uuid::new_v4().to_string();
        let lease_until = db::now() + item.spec.timeout_seconds as i64 + 30;
        tx.execute("INSERT INTO remote_runs(item_id,run_id,worker,revision,lease_until,result_hash) VALUES(?,?,?,?,?,NULL) ON CONFLICT(item_id) DO UPDATE SET run_id=excluded.run_id,worker=excluded.worker,revision=excluded.revision,lease_until=excluded.lease_until,result_hash=NULL", params![item.id,run_id,node,item.revision,lease_until])?;
        let assignment = Assignment {
            protocol: 1,
            run_id,
            item_id: item.id,
            revision: item.revision,
            spec: item.spec,
            lease_until,
        };
        tx.commit()?;
        return Ok(Some(assignment));
    }
    tx.commit()?;
    Ok(None)
}

pub fn complete(
    conn: &mut rusqlite::Connection,
    node: &str,
    allow_scripts: bool,
    result: &Completion,
) -> Result<bool> {
    ensure!(
        result.protocol == 1 && result.run_id.len() <= 64 && result.item_id.len() <= 64,
        "invalid worker protocol"
    );
    let obs = &result.observation;
    ensure!(
        matches!(obs.status.as_str(), "operational" | "degraded" | "outage")
            && (0..=600_000).contains(&obs.latency_ms)
            && obs.output.len() <= 131072
            && obs.detail.len() <= 4096,
        "invalid worker observation"
    );
    let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(obs)?));
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let row: Option<(String,u64,i64,Option<String>)> = tx.query_row("SELECT run_id,revision,lease_until,result_hash FROM remote_runs WHERE item_id=? AND worker=?",params![result.item_id,node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((id, revision, until, accepted)) = row else {
        return Ok(false);
    };
    let item = db::items(&tx)?.into_iter().find(|item| {
        item.id == result.item_id
            && item.spec.enabled
            && item.spec.worker == node
            && item.revision == revision
    });
    let Some(item) = item else {
        return Ok(false);
    };
    if !allow_scripts && item.spec.kind != "http" {
        return Ok(false);
    }
    if id != result.run_id {
        return Ok(false);
    }
    if let Some(accepted) = accepted {
        return Ok(accepted == hash);
    }
    if until <= db::now() {
        return Ok(false);
    }
    db::record_transaction(&tx, &item, obs)?;
    tx.execute(
        "UPDATE remote_runs SET result_hash=? WHERE item_id=? AND run_id=?",
        params![hash, item.id, id],
    )?;
    tx.commit()?;
    Ok(true)
}

pub fn router() -> Router<crate::server::App> {
    Router::new()
        .route("/api/workers/{node}/claim", post(claim_handler))
        .route("/api/workers/{node}/result", post(result_handler))
}
async fn claim_handler(
    State(app): State<crate::server::App>,
    Path(node): Path<String>,
    headers: HeaderMap,
    Json(request): Json<Claim>,
) -> RemoteResult<Option<Assignment>> {
    let worker = authorize(&app.cfg, &node, &headers)?;
    if request.protocol != 1 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Unsupported worker protocol",
        ));
    }
    let result = claim(
        &mut app.db.lock().unwrap(),
        &node,
        worker.allow_scripts && request.allow_scripts,
    )
    .map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Worker assignment unavailable",
        )
    })?;
    Ok(Json(result))
}
async fn result_handler(
    State(app): State<crate::server::App>,
    Path(node): Path<String>,
    headers: HeaderMap,
    Json(result): Json<Completion>,
) -> RemoteResult<serde_json::Value> {
    let worker = authorize(&app.cfg, &node, &headers)?;
    let accepted = complete(
        &mut app.db.lock().unwrap(),
        &node,
        worker.allow_scripts,
        &result,
    )
    .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid worker result"))?;
    if !accepted {
        return Err(error(
            StatusCode::CONFLICT,
            "Expired or stale worker result",
        ));
    }
    Ok(Json(serde_json::json!({"accepted":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    pub control_url: String,
    pub node_id: String,
    pub token_env: String,
    #[serde(default)]
    pub probe_env: Vec<String>,
    pub secrets: Option<Secrets>,
    pub ca_file: Option<String>,
    #[serde(default)]
    pub allow_scripts: bool,
    #[serde(default)]
    pub allow_insecure_http: bool,
}
pub async fn run(path: &FilePath) -> Result<()> {
    let worker: WorkerConfig = toml::from_str(&std::fs::read_to_string(path)?)?;
    ensure!(valid_node(&worker.node_id), "invalid node ID");
    let url = reqwest::Url::parse(&worker.control_url)?;
    ensure!(
        url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "control_url must be an origin without credentials"
    );
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    ensure!(
        url.scheme() == "https"
            || (url.scheme() == "http" && (loopback || worker.allow_insecure_http)),
        "remote controller requires HTTPS"
    );
    let mut cfg = Config {
        probe_env: worker.probe_env,
        secrets: worker.secrets,
        ..Default::default()
    };
    if let Some(secrets) = &cfg.secrets {
        cfg.secret_values = crate::secrets::load(
            FilePath::new(&secrets.vault),
            FilePath::new(&secrets.key_file),
        )?;
    }
    cfg.validate()?;
    ensure!(
        !cfg.probe_env.contains(&worker.token_env),
        "admission token must not be exposed to probes"
    );
    let token = zeroize::Zeroizing::new(cfg.environment(&worker.token_env)?);
    ensure!(
        (32..=512).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic()),
        "invalid worker token"
    );
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(ca) = worker.ca_file {
        builder =
            builder.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(ca)?)?);
    }
    let client = builder.build()?;
    let base = worker.control_url.trim_end_matches('/');
    let mut stop = crate::supervisor::shutdown_listener()?;
    let work = async {
        let mut backoff = 1;
        loop {
            let attempt = client
                .post(format!("{base}/api/workers/{}/claim", worker.node_id))
                .bearer_auth(&*token)
                .json(&Claim {
                    protocol: 1,
                    allow_scripts: worker.allow_scripts,
                })
                .send()
                .await;
            let mut response = match attempt {
                Ok(response) if response.status().is_success() => {
                    backoff = 1;
                    response
                }
                Ok(response) if matches!(response.status().as_u16(), 401 | 403) => {
                    anyhow::bail!("worker admission rejected")
                }
                _ => {
                    tracing::warn!("worker controller unavailable; retrying");
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(30);
                    continue;
                }
            };
            let mut body = Vec::new();
            let mut read_failed = false;
            loop {
                let chunk = match response.chunk().await {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => break,
                    Err(_) => {
                        read_failed = true;
                        break;
                    }
                };
                ensure!(
                    body.len() + chunk.len() <= 512 * 1024,
                    "assignment too large"
                );
                body.extend_from_slice(&chunk);
            }
            if read_failed {
                tracing::warn!("worker assignment transfer interrupted; retrying");
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(30);
                continue;
            }
            let assignment: Option<Assignment> =
                serde_json::from_slice(&body).context("invalid controller assignment")?;
            let Some(assignment) = assignment else {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };
            ensure!(
                assignment.protocol == 1
                    && assignment.spec.worker == worker.node_id
                    && (worker.allow_scripts || assignment.spec.kind == "http"),
                "unsupported assignment"
            );
            assignment.spec.validate()?;
            let item = Item {
                id: assignment.item_id.clone(),
                spec: assignment.spec,
                revision: assignment.revision,
                status: "unknown".into(),
                last_check: None,
                latency_ms: None,
                failures: 0,
                successes: 0,
            };
            let mut observation = crate::engine::probe_with_config(&item, &cfg).await;
            if observation.detail.len() > 4096 {
                let mut length = 4096;
                while !observation.detail.is_char_boundary(length) {
                    length -= 1;
                }
                observation.detail.truncate(length);
            }
            let mut completion = Completion {
                protocol: 1,
                run_id: assignment.run_id,
                item_id: assignment.item_id,
                observation,
            };
            while serde_json::to_vec(&completion)?.len() > 120 * 1024 {
                let output = &mut completion.observation.output;
                ensure!(output.len() > 1024, "remote result metadata too large");
                let mut length = output.len() / 2;
                while !output.is_char_boundary(length) {
                    length -= 1;
                }
                output.truncate(length);
                output.push_str("\n[remote output truncated]");
            }
            for retry in 0..4 {
                let response = client
                    .post(format!("{base}/api/workers/{}/result", worker.node_id))
                    .bearer_auth(&*token)
                    .json(&completion)
                    .send()
                    .await;
                match response {
                    Ok(response)
                        if response.status().is_success()
                            || response.status() == StatusCode::CONFLICT =>
                    {
                        break;
                    }
                    Ok(response) if matches!(response.status().as_u16(), 401 | 403) => {
                        anyhow::bail!("worker admission rejected")
                    }
                    _ => {
                        tracing::warn!("worker result delivery unavailable; retrying");
                        tokio::time::sleep(Duration::from_secs(1 << retry)).await;
                    }
                }
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! { result=work => result, _=crate::supervisor::cancelled(&mut stop) => Ok(()) }
}

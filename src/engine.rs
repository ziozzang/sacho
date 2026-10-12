use crate::{
    config::Config,
    db::{self, Db, Item, Observation},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{Semaphore, watch},
};

const OUTPUT_LIMIT: usize = 65536;
async fn bounded_read<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let keep = n.min(OUTPUT_LIMIT.saturating_sub(result.len()));
        result.extend_from_slice(&buf[..keep]);
    }
    Ok(result)
}
struct GroupGuard(u32);
impl Drop for GroupGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}
async fn command(item: &Item, cfg: &Config) -> Result<(String, i32)> {
    let mut cmd = match item.spec.kind.as_str() {
        "shell" => {
            let mut c = Command::new("/bin/sh");
            c.args(["-c", &item.spec.target]);
            c
        }
        "python" => {
            let mut c = Command::new("python3");
            c.args(["-I", "-c", &item.spec.target]);
            c
        }
        "lua" => {
            let mut c = Command::new(std::env::current_exe()?);
            c.arg("probe-lua");
            c
        }
        _ => unreachable!(),
    };
    cmd.env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in &cfg.probe_names() {
        if let Ok(value) = cfg.environment(name) {
            cmd.env(name, value);
        }
    }
    cmd.env(
        "SACHO_PROBE_ALLOWED_ENV",
        serde_json::to_string(&cfg.probe_names())?,
    )
    .env("SACHO_PROBE_TIMEOUT", item.spec.timeout_seconds.to_string());
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().context("start probe interpreter")?;
    let _guard = GroupGuard(child.id().context("probe has no process ID")?);
    let mut stdin = child.stdin.take().unwrap();
    if item.spec.kind == "lua" {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(item.spec.target.as_bytes()).await?;
    }
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(item.spec.timeout_seconds), async {
        let (status, stdout, stderr) =
            tokio::try_join!(child.wait(), bounded_read(stdout), bounded_read(stderr))?;
        let mut output = String::from_utf8_lossy(&stdout).into_owned();
        if !stderr.is_empty() {
            output.push_str("\n[stderr]\n");
            output.push_str(&String::from_utf8_lossy(&stderr));
        }
        Ok::<_, std::io::Error>((output, status.code().unwrap_or(-1)))
    })
    .await;
    match result {
        Ok(result) => Ok(result?),
        Err(_) => {
            let _ = child.kill().await;
            anyhow::bail!("probe timed out after {}s", item.spec.timeout_seconds);
        }
    }
}
pub fn lua_probe(source: &str) -> Result<()> {
    use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Value, VmState};
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    lua.set_memory_limit(16 * 1024 * 1024)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let started = Instant::now();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(10000),
        move |_, _| {
            if started.elapsed() > Duration::from_secs(300) {
                Err(mlua::Error::RuntimeError("Lua time limit".into()))
            } else {
                Ok(VmState::Continue)
            }
        },
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    crate::lua_api::install(&lua)?;
    let value: Value = lua
        .load(source)
        .eval()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    match value {
        Value::Boolean(true) => println!("true"),
        Value::Boolean(false) => anyhow::bail!("Lua returned false"),
        Value::Integer(n) => println!("{n}"),
        Value::Number(n) => println!("{n}"),
        Value::String(s) => println!("{}", s.to_string_lossy()),
        Value::Nil => println!("null"),
        _ => anyhow::bail!("Lua must return boolean, number, string or nil"),
    }
    Ok(())
}
pub async fn probe(item: &Item) -> Observation {
    probe_with_config(item, &Config::default()).await
}
pub async fn probe_with_config(item: &Item, cfg: &Config) -> Observation {
    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(item.spec.timeout_seconds), async {
        let (output, code) = if matches!(item.spec.kind.as_str(), "http" | "prometheus") {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(item.spec.timeout_seconds))
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let url = if item.spec.kind == "prometheus" {
                format!("{}/api/v1/query", item.spec.target.trim_end_matches('/'))
            } else {
                item.spec.target.clone()
            };
            let mut request = client.get(url);
            if item.spec.kind == "prometheus" {
                request = request.query(&[("query", &item.spec.prometheus_query)]);
            }
            for (header, env) in &item.spec.headers_env {
                ensure!(
                    cfg.probe_names().contains(env),
                    "HTTP credential environment name is not allowed"
                );
                request = request.header(
                    header,
                    cfg.environment(env).context("probe credential missing")?,
                );
            }
            let mut response = request.send().await?;
            let status = response.status().as_u16();
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(
                    body.len() + chunk.len() <= OUTPUT_LIMIT,
                    "HTTP response exceeds 64 KiB"
                );
                body.extend_from_slice(&chunk);
            }
            let output = if item.spec.kind == "prometheus" && status == 200 {
                crate::sources::prometheus(&body)?
            } else {
                String::from_utf8_lossy(&body).into_owned()
            };
            (output, status as i32)
        } else if item.spec.kind == "tcp" {
            (crate::sources::tcp(&item.spec.target).await?, 0)
        } else if item.spec.kind == "dns" {
            (crate::sources::dns(&item.spec).await?, 0)
        } else if item.spec.kind == "push" {
            anyhow::bail!(
                "Push checks accept authenticated reports; draft execution is unavailable"
            )
        } else {
            command(item, cfg).await?
        };
        let expected = if matches!(item.spec.kind.as_str(), "http" | "prometheus") {
            item.spec.expected_status as i32
        } else {
            item.spec.expected_exit
        };
        let mut detail = if code == expected {
            String::new()
        } else {
            format!("expected result {expected}, received {code}")
        };
        if detail.is_empty()
            && item.spec.kind == "dns"
            && serde_json::from_str::<serde_json::Value>(&output)?["healthy"] == 0
        {
            detail = "DNS answer set does not match expected records".into();
        }
        if detail.is_empty()
            && !item.spec.contains.is_empty()
            && !output.contains(&item.spec.contains)
        {
            detail = "expected text missing".into();
        }
        if detail.is_empty()
            && let Some(threshold) = item.spec.threshold
        {
            match crate::monitoring::number(&output, &item.spec.json_pointer) {
                Some(n) if !compare(n, threshold, &item.spec.comparison) => {
                    detail = format!(
                        "numeric condition failed: {n} {} {threshold}",
                        item.spec.comparison
                    );
                }
                Some(_) => {}
                None => {
                    detail =
                        "numeric condition requires a finite JSON number at its pointer".into();
                }
            }
        }
        Ok::<_, anyhow::Error>((output, detail))
    })
    .await;
    let latency_ms = start.elapsed().as_millis().min(i64::MAX as u128) as i64;
    match result {
        Ok(Ok((mut output, detail))) => {
            while output.len() > OUTPUT_LIMIT {
                output.pop();
            }
            let mut status = if !detail.is_empty() {
                item.spec.failure_status.as_str()
            } else if item
                .spec
                .degraded_ms
                .is_some_and(|limit| latency_ms as u64 > limit)
            {
                "degraded"
            } else {
                "operational"
            };
            let mut detail = if status == "degraded" && detail.is_empty() {
                "response time exceeded degraded threshold".into()
            } else {
                detail
            };
            if let Some(rules) = &item.spec.value_rules {
                if let Some(value) = crate::monitoring::number(&output, &rules.pointer) {
                    let severity = rules.severity(value);
                    if status != "outage"
                        && (severity == "outage"
                            || (severity == "degraded" && status == "operational"))
                    {
                        status = severity;
                        detail = format!("value threshold crossed: {value}");
                    }
                } else if status == "operational" {
                    status = item.spec.failure_status.as_str();
                    detail = "value rule requires a finite JSON number".into();
                }
            }
            Observation {
                status: status.into(),
                latency_ms,
                output,
                detail,
            }
        }
        Ok(Err(error)) => Observation {
            status: item.spec.failure_status.clone(),
            latency_ms,
            output: String::new(),
            detail: safe_error(&error, item),
        },
        Err(_) => Observation {
            status: item.spec.failure_status.clone(),
            latency_ms,
            output: String::new(),
            detail: format!("probe timed out after {}s", item.spec.timeout_seconds),
        },
    }
}
fn safe_error(error: &anyhow::Error, item: &Item) -> String {
    if matches!(item.spec.kind.as_str(), "http" | "prometheus") {
        "HTTP request or condition failed; verify connectivity, TLS and output format".into()
    } else {
        format!("{error:#}").chars().take(2000).collect()
    }
}
pub fn compare(value: f64, threshold: f64, operator: &str) -> bool {
    match operator {
        "lt" => value < threshold,
        "lte" => value <= threshold,
        "gt" => value > threshold,
        "gte" => value >= threshold,
        "eq" => value == threshold,
        _ => false,
    }
}
pub async fn scheduler(db: Db, cfg: Arc<Config>, mut stop: watch::Receiver<bool>) {
    let semaphore = Arc::new(Semaphore::new(cfg.max_concurrent_checks));
    let mut running = HashSet::new();
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut prune = 0;
    loop {
        tokio::select! {
            _=stop.changed()=>break,
            Some(id)=done_rx.recv()=>{running.remove(&id);},
            _=tick.tick()=>{
                let all={db::items(&db.lock().unwrap())};
                match all {
                    Ok(items)=>for item in items {
                        if !item.spec.worker.is_empty() || !item.spec.enabled || running.contains(&item.id) || !crate::scheduling::due(&db.lock().unwrap(),&item,db::now()).unwrap_or(false) { continue; }
                        if item.spec.kind=="push" {
                            if let Err(error)=crate::push::expire(&mut db.lock().unwrap(),&item) {tracing::error!(%error,"expire push report");}
                            continue;
                        }
                        let Ok(permit)=semaphore.clone().try_acquire_owned() else {break;};
                        running.insert(item.id.clone());
                        let db=db.clone(); let cfg=cfg.clone(); let done=done_tx.clone(); let mut cancel=stop.clone();
                        tokio::spawn(async move {
                            let _permit=permit;
                            tokio::select! {
                                _=cancel.changed()=>{},
                                obs=probe_with_config(&item,&cfg)=>{ if let Err(error)=db::record(&mut db.lock().unwrap(),&item,&obs) {tracing::error!(%error,"record observation");} }
                            }
                            let _=done.send(item.id);
                        });
                    },
                    Err(error)=>tracing::error!(%error,"load checks"),
                }
                prune+=1;
                if prune>=3600 {
                    prune=0;
                    let conn=db.lock().unwrap();
                    let cutoff=db::now()-cfg.retention_days as i64*86400;
                    for sql in ["DELETE FROM checks WHERE at < ?", "DELETE FROM outbox WHERE created_at < ? AND (delivered_at IS NOT NULL OR attempts >= 8)","DELETE FROM subscribers WHERE verified=0 AND created_at < ?"] { let _=conn.execute(sql,[cutoff]); }
                }
            }
        }
    }
}

pub fn validate_lua(source: &str) -> Result<()> {
    let lua = mlua::Lua::new();
    lua.load(source)
        .into_function()
        .map_err(|_| anyhow::anyhow!("Lua syntax is invalid"))?;
    Ok(())
}

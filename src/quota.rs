//! Read-only usage adapters. No inference requests, credential writes or refreshes.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

pub fn read(provider: &str, home: &Path, codex_binary: &str) -> Result<Value> {
    match provider {
        "codex" => codex(codex_binary),
        "claude" => claude(home),
        _ => anyhow::bail!("usage provider must be codex or claude"),
    }
}
fn window(
    value: &Value,
    percent_key: &str,
    minutes_key: &str,
    default_minutes: u64,
) -> Option<Value> {
    let used = value[percent_key].as_f64()?;
    if !(0.0..=100.0).contains(&used) {
        return None;
    }
    let duration = value[minutes_key].as_u64().unwrap_or(default_minutes);
    Some(
        json!({"remaining_percent":100.0-used,"used_percent":used,"window_minutes":duration,"resets_at":value.get("resetsAt").or_else(||value.get("resets_at"))}),
    )
}
fn summary(provider: &str, windows: Vec<Value>) -> Result<Value> {
    ensure!(!windows.is_empty(), "no reported quota windows");
    let remaining = windows
        .iter()
        .filter_map(|w| w["remaining_percent"].as_f64())
        .fold(100.0_f64, f64::min);
    let display = windows
        .iter()
        .map(|w| {
            let mins = w["window_minutes"].as_u64().unwrap_or(0);
            let duration = if mins >= 1440 {
                format!("{}d", mins / 1440)
            } else if mins >= 60 {
                format!("{}h", mins / 60)
            } else {
                format!("{mins}m")
            };
            format!(
                "{duration} {:.0}%",
                w["remaining_percent"].as_f64().unwrap_or(0.0)
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    Ok(
        json!({"provider":provider,"remaining_percent":remaining,"display":display,"windows":windows,"updated_at":crate::db::now()}),
    )
}
fn codex(binary: &str) -> Result<Value> {
    let mut command = Command::new(binary);
    command
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .context("Codex app-server is not available")?;
    let result = (|| -> Result<Value> {
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout.take(2 * 1024 * 1024));
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && value.get("id").is_some()
                    && tx.send(value).is_err()
                {
                    break;
                }
            }
        });
        let input = child.stdin.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"sacho_usage","version":crate::VERSION}}})
        )?;
        input.flush()?;
        let init = rx
            .recv_timeout(Duration::from_secs(10))
            .context("Codex initialization timed out")?;
        ensure!(
            init["id"] == 1 && init.get("error").is_none(),
            "Codex initialization failed"
        );
        writeln!(input, "{}", json!({"method":"initialized","params":{}}))?;
        writeln!(
            input,
            "{}",
            json!({"id":2,"method":"account/rateLimits/read"})
        )?;
        input.flush()?;
        let response = rx
            .recv_timeout(Duration::from_secs(15))
            .context("Codex usage read timed out")?;
        ensure!(
            response["id"] == 2 && response.get("error").is_none(),
            "Codex usage not available for current authentication"
        );
        let result = &response["result"];
        let bucket = result["rateLimitsByLimitId"]
            .get("codex")
            .filter(|v| !v.is_null())
            .unwrap_or(&result["rateLimits"]);
        let windows = ["primary", "secondary"]
            .iter()
            .filter_map(|key| window(&bucket[*key], "usedPercent", "windowDurationMins", 0))
            .collect();
        summary("codex", windows)
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}
fn claude(home: &Path) -> Result<Value> {
    let file = std::fs::File::open(home.join(".claude/.credentials.json"))
        .context("Claude OAuth credentials are not available")?;
    let credentials: Value =
        serde_json::from_reader(file.take(64 * 1024)).context("invalid Claude credential file")?;
    let oauth = &credentials["claudeAiOauth"];
    let token = oauth["accessToken"]
        .as_str()
        .context("Claude subscription OAuth token is missing")?;
    ensure!(
        !token.starts_with("sk-ant-api"),
        "Claude usage requires a subscription OAuth token"
    );
    if let Some(expires) = oauth["expiresAt"].as_i64() {
        ensure!(
            expires > crate::db::now() * 1000,
            "Claude OAuth token has expired; sign in with Claude again"
        );
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(token)
        .header("Anthropic-Beta", "oauth-2025-04-20")
        .header("User-Agent", concat!("sacho/", env!("CARGO_PKG_VERSION")))
        .send()
        .context("Claude usage connection failed")?;
    ensure!(
        response.status().is_success(),
        "Claude usage returned HTTP {}",
        response.status()
    );
    let data: Value =
        serde_json::from_reader(response.take(65536)).context("invalid Claude usage response")?;
    let windows = [("five_hour", 300), ("seven_day", 10080)]
        .into_iter()
        .filter_map(|(key, minutes)| window(&data[key], "utilization", "window_minutes", minutes))
        .collect();
    summary("claude", windows)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_and_null_windows_are_not_full_allowances() {
        assert!(window(&json!(null), "usedPercent", "windowDurationMins", 0).is_none());
        assert!(summary("codex", vec![]).is_err());
        assert!(
            window(
                &json!({"usedPercent":101}),
                "usedPercent",
                "windowDurationMins",
                0
            )
            .is_none()
        );
    }
    #[test]
    fn summary_preserves_window_scope() {
        let a = window(&json!({"utilization":10}), "utilization", "minutes", 300).unwrap();
        let b = window(&json!({"utilization":80}), "utilization", "minutes", 10080).unwrap();
        let r = summary("claude", vec![a, b]).unwrap();
        assert_eq!(r["remaining_percent"].as_f64(), Some(20.0));
        assert_eq!(r["display"], "5h 90% · 7d 20%");
    }
}

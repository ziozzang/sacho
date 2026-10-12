use anyhow::Result;
use mlua::{Lua, LuaSerdeExt};
use std::{
    io::Read,
    time::{Duration, Instant},
};
fn err(message: &str) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}
fn install_inner(lua: &Lua) -> mlua::Result<()> {
    let host = lua.create_table()?;
    host.set("api_version", 1)?;
    host.set(
        "exec",
        lua.create_function(|lua, arguments: mlua::Table| {
            use std::process::{Command, Stdio};
            let arguments = arguments
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()?;
            if arguments.is_empty()
                || arguments.len() > 64
                || arguments.iter().map(String::len).sum::<usize>() > 16384
            {
                return Err(err("exec expects 1–64 command arguments, up to 16 KiB"));
            }
            let mut child = Command::new(&arguments[0])
                .args(&arguments[1..])
                .env_remove("SACHO_PROBE_ALLOWED_ENV")
                .env_remove("SACHO_PROBE_TIMEOUT")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|_| err("external command could not start"))?;
            fn capture(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
                let mut result = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let count = reader.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    let keep = count.min(65536_usize.saturating_sub(result.len()));
                    result.extend_from_slice(&buffer[..keep]);
                }
                Ok(result)
            }
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let output = std::thread::spawn(move || capture(stdout));
            let errors = std::thread::spawn(move || capture(stderr));
            let timeout = std::env::var("SACHO_PROBE_TIMEOUT")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(10)
                .clamp(1, 300);
            let started = Instant::now();
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) if started.elapsed() < Duration::from_secs(timeout) => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(err("external command failed or timed out"));
                    }
                }
            };
            let stdout = output
                .join()
                .map_err(|_| err("external stdout capture failed"))?
                .map_err(|_| err("external stdout read failed"))?;
            let stderr = errors
                .join()
                .map_err(|_| err("external stderr capture failed"))?
                .map_err(|_| err("external stderr read failed"))?;
            let result = lua.create_table()?;
            result.set("exit_code", status.code().unwrap_or(-1))?;
            result.set("stdout", lua.create_string(&stdout)?)?;
            result.set("stderr", lua.create_string(&stderr)?)?;
            Ok(result)
        })?,
    )?;
    let allowed: Vec<String> = serde_json::from_str(
        &std::env::var("SACHO_PROBE_ALLOWED_ENV").unwrap_or_else(|_| "[]".into()),
    )
    .map_err(mlua::Error::external)?;
    host.set(
        "env",
        lua.create_function(move |_, name: String| {
            if !allowed.contains(&name) {
                return Err(err("environment variable is not in probe_env allowlist"));
            }
            std::env::var(name).map_err(|_| err("allowed environment variable missing"))
        })?,
    )?;
    host.set(
        "json_decode",
        lua.create_function(|lua, text: String| {
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|_| err("invalid JSON"))?;
            lua.to_value(&value)
        })?,
    )?;
    host.set(
        "json_encode",
        lua.create_function(|lua, value: mlua::Value| {
            let value: serde_json::Value = lua.from_value(value)?;
            serde_json::to_string(&value).map_err(mlua::Error::external)
        })?,
    )?;
    let timeout = std::env::var("SACHO_PROBE_TIMEOUT")
        .ok()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 300);
    host.set(
        "http",
        lua.create_function(move |lua, options: mlua::Value| {
            let started = Instant::now();
            let (url, method, headers, body) = match options {
                mlua::Value::String(url) => {
                    (url.to_str()?.to_string(), "GET".to_string(), None, None)
                }
                mlua::Value::Table(t) => (
                    t.get::<String>("url")?,
                    t.get::<Option<String>>("method")?
                        .unwrap_or_else(|| "GET".into()),
                    t.get::<Option<mlua::Table>>("headers")?,
                    t.get::<Option<String>>("body")?,
                ),
                _ => return Err(err("http expects a URL or options table")),
            };
            let url = reqwest::Url::parse(&url).map_err(|_| err("invalid HTTP URL"))?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(err("HTTP(S) URL without credentials required"));
            }
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(timeout))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| err("HTTP client failed"))?;
            let method = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|_| err("invalid HTTP method"))?;
            let mut request = client.request(method, url);
            if let Some(headers) = headers {
                for pair in headers.pairs::<String, String>() {
                    let (k, v) = pair?;
                    request = request.header(k, v);
                }
            }
            if let Some(body) = body {
                if body.len() > 65536 {
                    return Err(err("HTTP body exceeds 64 KiB"));
                }
                request = request.body(body);
            }
            let mut response = request
                .send()
                .map_err(|_| err("HTTP request failed (connectivity, TLS or timeout)"))?;
            let status = response.status().as_u16();
            let mut bytes = Vec::new();
            response
                .by_ref()
                .take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| err("HTTP response read failed"))?;
            if bytes.len() > 65536 {
                return Err(err("HTTP response exceeds 64 KiB"));
            }
            let table = lua.create_table()?;
            table.set("status", status)?;
            table.set("body", lua.create_string(&bytes)?)?;
            table.set("latency_ms", started.elapsed().as_millis() as u64)?;
            Ok(table)
        })?,
    )?;
    lua.globals().set("sacho", host)?;
    Ok(())
}
pub fn install(lua: &Lua) -> Result<()> {
    install_inner(lua).map_err(|e| anyhow::anyhow!(e.to_string()))
}

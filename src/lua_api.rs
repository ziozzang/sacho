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
        "usage",
        lua.create_function(|lua, provider: String| {
            let home = std::env::var("SACHO_PROBE_USAGE_HOME")
                .map_err(|_| err("local usage adapters disabled; configure [usage]"))?;
            let binary =
                std::env::var("SACHO_PROBE_CODEX_BINARY").unwrap_or_else(|_| "codex".into());
            let value = crate::quota::read(&provider, std::path::Path::new(&home), &binary)
                .map_err(|e| err(&e.to_string()))?;
            lua.to_value(&value)
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

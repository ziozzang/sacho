//! Administrative probe inputs. API responses never contain stored values.
use crate::{
    config::Config,
    server::{ApiError, App},
};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::{Path, State},
};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path as FilePath;
const KEY: &str = "probe.input_names";

pub fn restore(app: &App) -> Result<()> {
    let value: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT value FROM settings WHERE key=?", [KEY], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(value) = value {
        let names: Vec<String> = serde_json::from_str(&value)?;
        ensure!(
            names.len() <= 64 && names.iter().all(|n| app.cfg.manageable_input(n)),
            "invalid saved probe input names"
        );
        *app.cfg.runtime_inputs.write().unwrap() = names;
    }
    Ok(())
}
fn values(cfg: &Config) -> Result<std::collections::BTreeMap<String, String>> {
    match &cfg.secrets {
        Some(p) => crate::secrets::load(FilePath::new(&p.vault), FilePath::new(&p.key_file)),
        None => Ok(Default::default()),
    }
}
pub async fn list(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    let stored = values(&app.cfg)?;
    let allowed = app.cfg.probe_names();
    let mut names: std::collections::BTreeSet<String> = allowed.iter().cloned().collect();
    names.extend(
        stored
            .keys()
            .filter(|n| app.cfg.manageable_input(n))
            .cloned(),
    );
    Ok(Json(
        json!({"vault_configured":app.cfg.secrets.is_some(),"scope":"local","inputs":names.into_iter().map(|name| {
        let environment=std::env::var(&name).is_ok();
        json!({"name":name,"enabled":allowed.contains(&name.to_string()),"configured":app.cfg.probe_env.contains(&name.to_string()),"stored":stored.contains_key(&name),"source":if environment {"environment"} else if stored.contains_key(&name) {"vault"} else {"missing"},"editable":app.cfg.manageable_input(&name)})
    }).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputUpdate {
    value: Option<String>,
    enabled: bool,
}
pub async fn put(
    State(app): State<App>,
    Path(name): Path<String>,
    Json(input): Json<InputUpdate>,
) -> Result<Json<Value>, ApiError> {
    put_impl(&app, &name, input).map(Json).map_err(Into::into)
}
fn put_impl(app: &App, name: &str, input: InputUpdate) -> Result<Value> {
    ensure!(
        app.cfg.manageable_input(name),
        "reserved or invalid input name"
    );
    ensure!(
        input.enabled || !app.cfg.probe_env.contains(&name.to_string()),
        "configured allowlist names must be changed in server configuration"
    );
    let paths =
        app.cfg.secrets.as_ref().ok_or_else(|| {
            anyhow::anyhow!("configure and initialize vault before managing inputs")
        })?;
    // Do not grant access to an arbitrary inherited service credential.
    ensure!(
        std::env::var(name).is_err() || app.cfg.probe_env.contains(&name.to_string()),
        "inherited inputs require explicit server allowlist configuration"
    );
    let mut names = app.cfg.runtime_inputs.write().unwrap();
    let mut next = names.clone();
    next.retain(|n| n != name);
    if input.enabled && !app.cfg.probe_env.contains(&name.to_string()) {
        next.push(name.to_string());
    }
    ensure!(next.len() <= 64, "too many runtime inputs");
    if let Some(value) = input.value {
        crate::secrets::update(
            FilePath::new(&paths.vault),
            FilePath::new(&paths.key_file),
            name,
            Some(value),
        )?;
    }
    ensure!(
        !input.enabled || app.cfg.environment(name).is_ok(),
        "input has no available value"
    );
    app.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![KEY,serde_json::to_string(&next)?])?;
    *names = next;
    Ok(json!({"ok":true}))
}
pub async fn remove(
    State(app): State<App>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    remove_impl(&app, &name).map(Json).map_err(Into::into)
}
fn remove_impl(app: &App, name: &str) -> Result<Value> {
    ensure!(
        app.cfg.manageable_input(name),
        "reserved or invalid input name"
    );
    let paths = app
        .cfg
        .secrets
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("vault is not configured"))?;
    let mut names = app.cfg.runtime_inputs.write().unwrap();
    let next: Vec<_> = names.iter().filter(|n| *n != name).cloned().collect();
    // Revoke dynamic grants first. Server allowlists remain controlled by configuration.
    app.db.lock().unwrap().execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![KEY,serde_json::to_string(&next)?])?;
    *names = next;
    crate::secrets::update(
        FilePath::new(&paths.vault),
        FilePath::new(&paths.key_file),
        name,
        None,
    )?;
    Ok(json!({"ok":true}))
}

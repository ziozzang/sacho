//! Global local inputs; encryption is explicit and secrets never have readback.
use crate::{
    config::Config,
    server::{ApiError, App},
};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::{Path, State},
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path as FilePath,
};
const KEY: &str = "probe.input_names";
const LITERALS: &str = "probe.literal_values";
pub fn restore(app: &App) -> Result<()> {
    let conn = app.db.lock().unwrap();
    let value: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?", [KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let plain: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?", [LITERALS], |r| {
            r.get(0)
        })
        .optional()?;
    drop(conn);
    if let Some(value) = value {
        let names: Vec<String> = serde_json::from_str(&value)?;
        ensure!(
            names.len() <= 64 && names.iter().all(|n| app.cfg.manageable_input(n)),
            "invalid saved probe input names"
        );
        *app.cfg.runtime_inputs.write().unwrap() = names;
    }
    if let Some(plain) = plain {
        let values: BTreeMap<String, String> = serde_json::from_str(&plain)?;
        validate_literals(&values)?;
        ensure!(
            values.keys().all(|n| app.cfg.manageable_input(n)),
            "reserved plaintext input"
        );
        *app.cfg.runtime_literals.write().unwrap() = values;
    }
    Ok(())
}
fn validate_literals(values: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        values.len() <= 64
            && values.iter().all(|(n, v)| crate::secrets::valid_name(n)
                && v.len() <= 65536
                && !v.contains('\0'))
            && serde_json::to_vec(values)?.len() <= 262144,
        "plaintext inputs exceed limits"
    );
    Ok(())
}
fn values(cfg: &Config) -> Result<BTreeMap<String, String>> {
    match &cfg.secrets {
        Some(p) => crate::secrets::load(FilePath::new(&p.vault), FilePath::new(&p.key_file)),
        None => Ok(Default::default()),
    }
}
pub async fn list(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    let stored = values(&app.cfg)?;
    let plain = app.cfg.runtime_literals.read().unwrap().clone();
    let allowed = app.cfg.probe_names();
    let mut names: BTreeSet<String> = allowed.iter().cloned().collect();
    names.extend(
        stored
            .keys()
            .filter(|n| app.cfg.manageable_input(n))
            .cloned(),
    );
    names.extend(plain.keys().cloned());
    Ok(Json(
        json!({"vault_configured":app.cfg.secrets.is_some(),"scope":"local","inputs":names.into_iter().map(|name|{
        let environment=std::env::var(&name).is_ok();let literal=plain.get(&name);
        json!({"name":name,"enabled":allowed.contains(&name),"configured":app.cfg.probe_env.contains(&name),"stored":stored.contains_key(&name)||literal.is_some(),"encrypted":literal.is_none(),"value":literal,"source":if environment{"environment"}else if literal.is_some(){"literal"}else if stored.contains_key(&name){"vault"}else{"missing"},"editable":app.cfg.manageable_input(&name)})
    }).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputUpdate {
    value: Option<String>,
    enabled: bool,
    #[serde(default = "default_encryption")]
    encrypted: bool,
}
fn default_encryption() -> bool {
    true
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
        "server allowlist entries are configured outside this API"
    );
    ensure!(
        std::env::var(name).is_err() || app.cfg.probe_env.contains(&name.to_string()),
        "inherited inputs require explicit server configuration"
    );
    let stored = values(&app.cfg)?;
    let mut names = app.cfg.runtime_inputs.write().unwrap();
    let mut literals = app.cfg.runtime_literals.write().unwrap();
    let mut next_literals = literals.clone();
    let was_plain = next_literals.contains_key(name);
    ensure!(
        input.value.is_some()
            || input.encrypted != was_plain
            || (!was_plain && !stored.contains_key(name)),
        "supply a new value when changing encryption mode"
    );
    let mut next = names.clone();
    next.retain(|n| n != name);
    if input.enabled && !app.cfg.probe_env.contains(&name.to_string()) {
        next.push(name.into());
    }
    ensure!(next.len() <= 64, "too many inputs");
    if input.encrypted {
        if let Some(value) = input.value.as_ref() {
            let p = app
                .cfg
                .secrets
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("initialize a vault to encrypt inputs"))?;
            crate::secrets::update(
                FilePath::new(&p.vault),
                FilePath::new(&p.key_file),
                name,
                Some(value.clone()),
            )?;
        }
        next_literals.remove(name);
    } else if let Some(value) = input.value.as_ref() {
        next_literals.insert(name.into(), value.clone());
    }
    validate_literals(&next_literals)?;
    ensure!(
        !input.enabled
            || std::env::var(name).is_ok()
            || next_literals.contains_key(name)
            || input.encrypted && (stored.contains_key(name) || input.value.is_some()),
        "input has no available value"
    );
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![KEY,serde_json::to_string(&next)?])?;
    tx.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![LITERALS,serde_json::to_string(&next_literals)?])?;
    tx.commit()?;
    drop(conn);
    *names = next;
    *literals = next_literals;
    drop(literals);
    drop(names);
    if !input.encrypted
        && stored.contains_key(name)
        && let Some(p) = &app.cfg.secrets
    {
        crate::secrets::update(
            FilePath::new(&p.vault),
            FilePath::new(&p.key_file),
            name,
            None,
        )?;
    }
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
    let stored = values(&app.cfg)?;
    let mut names = app.cfg.runtime_inputs.write().unwrap();
    let mut plain = app.cfg.runtime_literals.write().unwrap();
    ensure!(
        stored.contains_key(name) || plain.contains_key(name),
        "input not found"
    );
    let next: Vec<_> = names.iter().filter(|n| *n != name).cloned().collect();
    let mut literals = plain.clone();
    literals.remove(name);
    let mut conn = app.db.lock().unwrap();
    let tx = conn.transaction()?;
    for (key, value) in [
        (KEY, serde_json::to_string(&next)?),
        (LITERALS, serde_json::to_string(&literals)?),
    ] {
        tx.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
    }
    tx.commit()?;
    drop(conn);
    *names = next;
    *plain = literals;
    drop(names);
    drop(plain);
    if stored.contains_key(name) {
        let p = app.cfg.secrets.as_ref().unwrap();
        crate::secrets::update(
            FilePath::new(&p.vault),
            FilePath::new(&p.key_file),
            name,
            None,
        )?;
    }
    Ok(json!({"ok":true}))
}

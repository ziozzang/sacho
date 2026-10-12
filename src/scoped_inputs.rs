//! Per-monitor inputs. Secret updates are transient; only vault references reach SQLite.
use crate::{
    config::Config,
    db::{self, CheckSpec},
    server::App,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub encrypted: bool,
    pub value: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub name: String,
    pub value: String,
    pub encrypted: bool,
}
impl std::fmt::Debug for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputUpdate")
            .field("name", &self.name)
            .field("encrypted", &self.encrypted)
            .field("value", &"[redacted]")
            .finish()
    }
}
pub fn validate(spec: &CheckSpec) -> Result<()> {
    ensure!(
        spec.input_bindings.len() <= 32 && spec.input_updates.len() <= 32,
        "at most 32 check inputs"
    );
    let valid = |name: &str, value: &str| {
        crate::secrets::valid_name(name)
            && !name.starts_with("SACHO_ITEM_")
            && value.len() <= 65536
            && !value.contains('\0')
    };
    ensure!(
        spec.input_bindings.iter().all(
            |(n, b)| valid(n, &b.value) && (!b.encrypted || b.value.starts_with("SACHO_ITEM_"))
        ),
        "invalid check input binding"
    );
    ensure!(
        spec.input_updates.iter().all(|u| valid(&u.name, &u.value)),
        "invalid check input update"
    );
    let names: std::collections::BTreeSet<_> = spec.input_updates.iter().map(|u| &u.name).collect();
    ensure!(
        names.len() == spec.input_updates.len(),
        "duplicate check input updates"
    );
    ensure!(
        spec.worker.is_empty()
            || (!spec.input_bindings.values().any(|b| b.encrypted)
                && !spec.input_updates.iter().any(|u| u.encrypted)),
        "remote secrets must be configured in the agent vault, not the controller"
    );
    Ok(())
}
pub fn prepare_probe(spec: &CheckSpec, cfg: &Config) -> Result<Config> {
    let mut scoped = cfg.clone();
    let mut bindings = spec.input_bindings.clone();
    for u in &spec.input_updates {
        bindings.insert(
            u.name.clone(),
            Binding {
                encrypted: false,
                value: u.value.clone(),
            },
        );
    }
    let encrypted = if bindings.values().any(|b| b.encrypted) {
        let p = cfg
            .secrets
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("check vault is not configured"))?;
        crate::secrets::load(Path::new(&p.vault), Path::new(&p.key_file))?
    } else {
        BTreeMap::new()
    };
    for (name, binding) in bindings {
        ensure!(cfg.manageable_input(&name), "reserved check input name");
        let value = if binding.encrypted {
            encrypted
                .get(&binding.value)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("encrypted check input is unavailable"))?
        } else {
            binding.value
        };
        if !scoped.probe_env.contains(&name) {
            scoped.probe_env.push(name.clone());
        }
        scoped.probe_overrides.insert(name, value);
    }
    Ok(scoped)
}
fn remove_keys(cfg: &Config, keys: impl Iterator<Item = String>) {
    if let Some(p) = &cfg.secrets {
        for key in keys {
            if crate::secrets::update(Path::new(&p.vault), Path::new(&p.key_file), &key, None)
                .is_err()
            {
                tracing::warn!("unused encrypted check input could not be removed");
            }
        }
    }
}
pub fn save(app: &App, id: Option<String>, mut spec: CheckSpec) -> Result<String> {
    spec.validate()?;
    let mut conn = app.db.lock().unwrap();
    let previous = if let Some(id) = &id {
        db::items(&conn)?
            .into_iter()
            .find(|i| &i.id == id)
            .ok_or_else(|| anyhow::anyhow!("monitor not found"))?
            .spec
            .input_bindings
    } else {
        BTreeMap::new()
    };
    for (name, binding) in &spec.input_bindings {
        ensure!(app.cfg.manageable_input(name), "reserved check input name");
        ensure!(
            !binding.encrypted || previous.get(name) == Some(binding),
            "secret bindings must belong to this monitor"
        );
    }
    let mut created = vec![];
    let outcome = (|| -> Result<String> {
        for update in &spec.input_updates {
            ensure!(
                app.cfg.manageable_input(&update.name),
                "reserved check input name"
            );
            let value = if update.encrypted {
                let p =
                    app.cfg.secrets.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("initialize a vault to encrypt check inputs")
                    })?;
                let key = format!("SACHO_ITEM_{}", uuid::Uuid::new_v4().simple());
                crate::secrets::update(
                    Path::new(&p.vault),
                    Path::new(&p.key_file),
                    &key,
                    Some(update.value.clone()),
                )?;
                created.push(key.clone());
                key
            } else {
                update.value.clone()
            };
            spec.input_bindings.insert(
                update.name.clone(),
                Binding {
                    encrypted: update.encrypted,
                    value,
                },
            );
        }
        spec.input_updates.clear();
        db::save(&mut conn, id, &spec)
    })();
    drop(conn);
    if outcome.is_err() {
        remove_keys(&app.cfg, created.into_iter());
    } else {
        remove_keys(
            &app.cfg,
            previous
                .values()
                .filter(|b| b.encrypted && !spec.input_bindings.values().any(|next| next == *b))
                .map(|b| b.value.clone()),
        );
    }
    outcome
}
pub fn remove_monitor_inputs(cfg: &Config, spec: &CheckSpec) {
    remove_keys(
        cfg,
        spec.input_bindings
            .values()
            .filter(|b| b.encrypted)
            .map(|b| b.value.clone()),
    );
}

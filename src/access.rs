//! Read access is evaluated from the trusted authentication proxy on every request.
use crate::{
    config::Config,
    db::{self, CheckSpec},
};
use anyhow::{Result, ensure};
use axum::http::HeaderMap;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub emails: Vec<String>,
}
pub fn default_visibility() -> String {
    "public".into()
}
pub fn is_public(s: &str) -> bool {
    s == "public"
}
pub fn validate(spec: &CheckSpec) -> Result<()> {
    ensure!(
        matches!(
            spec.visibility.as_str(),
            "public" | "authenticated" | "groups"
        ),
        "visibility must be public, authenticated or groups"
    );
    ensure!(
        spec.access_groups.len() <= 32 && spec.access_groups.iter().all(|g| valid_group(g)),
        "invalid access group names"
    );
    ensure!(
        spec.visibility != "groups" || !spec.access_groups.is_empty(),
        "group visibility requires at least one access group"
    );
    Ok(())
}
pub fn valid_group(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
pub fn email(cfg: &Config, headers: &HeaderMap) -> Option<String> {
    if cfg.auth.mode != "auth-proxy" {
        return None;
    }
    let values = headers.get_all(&cfg.auth.email_header);
    let mut values = values.iter();
    let value = values.next()?.to_str().ok()?.trim().to_lowercase();
    if values.next().is_some()
        || value.len() > 254
        || value.contains(['\r', '\n', '<', '>', ' ', ','])
        || value.parse::<lettre::Address>().is_err()
    {
        return None;
    }
    Some(value)
}
pub fn groups(conn: &Connection) -> Result<Vec<Group>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='access.groups'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(raw.as_deref().unwrap_or("[]"))?)
}
pub fn normalize(mut groups: Vec<Group>) -> Result<Vec<Group>> {
    ensure!(groups.len() <= 64, "at most 64 access groups");
    let mut names = HashSet::new();
    for group in &mut groups {
        ensure!(
            valid_group(&group.name) && names.insert(group.name.clone()),
            "invalid or duplicate access group name"
        );
        ensure!(group.emails.len() <= 128, "at most 128 members per group");
        for email in &mut group.emails {
            *email = email.trim().to_lowercase();
            ensure!(
                email.len() <= 254
                    && !email.contains(['\r', '\n', '<', '>', ' ', ','])
                    && email.parse::<lettre::Address>().is_ok(),
                "invalid group member email"
            );
        }
        group.emails.sort();
        group.emails.dedup();
    }
    Ok(groups)
}
pub struct Reader {
    email: Option<String>,
    admin: bool,
    memberships: HashSet<String>,
}
impl Reader {
    pub fn new(conn: &Connection, cfg: &Config, headers: &HeaderMap) -> Result<Self> {
        let email = email(cfg, headers);
        let admin = email
            .as_ref()
            .is_some_and(|e| cfg.auth.admins.iter().any(|a| a.eq_ignore_ascii_case(e)));
        let memberships = if let Some(email) = &email {
            groups(conn)?
                .into_iter()
                .filter(|g| g.emails.iter().any(|e| e.eq_ignore_ascii_case(email)))
                .map(|g| g.name)
                .collect()
        } else {
            HashSet::new()
        };
        Ok(Self {
            email,
            admin,
            memberships,
        })
    }
    pub fn allowed(&self, spec: &CheckSpec) -> bool {
        self.admin
            || match spec.visibility.as_str() {
                "public" => true,
                "authenticated" => self.email.is_some(),
                "groups" => {
                    self.email.is_some()
                        && spec
                            .access_groups
                            .iter()
                            .any(|g| self.memberships.contains(g))
                }
                _ => false,
            }
    }
}
pub fn visible_ids(conn: &Connection, reader: &Reader) -> Result<Vec<String>> {
    Ok(db::items(conn)?
        .into_iter()
        .filter(|i| i.spec.enabled && reader.allowed(&i.spec))
        .map(|i| i.id)
        .collect())
}
pub fn layout(site: &mut Value, ids: &HashSet<String>) {
    if let Some(sections) = site["layout"].as_array_mut() {
        for section in sections.iter_mut() {
            if let Some(items) = section["item_ids"].as_array_mut() {
                items.retain(|id| id.as_str().is_some_and(|id| ids.contains(id)));
            }
        }
        sections.retain(|s| s["item_ids"].as_array().is_some_and(|a| !a.is_empty()));
    }
}
pub fn incidents(conn: &Connection, reader: &Reader) -> Result<Vec<Value>> {
    let ids = serde_json::to_string(&visible_ids(conn, reader)?)?;
    let mut stmt=conn.prepare("SELECT id,item_id,name,status,opened_at,resolved_at FROM incidents WHERE item_id IN (SELECT value FROM json_each(?)) ORDER BY opened_at DESC LIMIT 100")?;
    Ok(stmt.query_map([ids],|r|Ok(json!({"id":r.get::<_,String>(0)?,"item_id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"opened_at":r.get::<_,i64>(4)?,"resolved_at":r.get::<_,Option<i64>>(5)?})))?.collect::<std::result::Result<Vec<_>,_>>()?)
}
pub fn public_event(conn: &Connection, event: &Value) -> Result<bool> {
    let Some(id) = event["item_id"].as_str() else {
        return Ok(false);
    };
    let raw: Option<String> = conn
        .query_row("SELECT spec FROM items WHERE id=?", [id], |r| r.get(0))
        .optional()?;
    Ok(raw
        .map(|raw| serde_json::from_str::<CheckSpec>(&raw))
        .transpose()?
        .is_some_and(|s| s.visibility == "public"))
}
pub fn matrix(conn: &Connection) -> Result<Value> {
    let groups = groups(conn)?;
    let counts: BTreeMap<String, usize> = groups
        .iter()
        .map(|g| (g.name.clone(), g.emails.len()))
        .collect();
    Ok(json!({"groups":groups,"member_counts":counts}))
}

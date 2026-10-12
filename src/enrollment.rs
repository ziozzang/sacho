//! One-use worker admission. The controller retains hashes, never bearer secrets.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const NODE_PREFIX: &str = "worker.node.";
const INVITE_PREFIX: &str = "worker.bootstrap.";
pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn get<T: serde::de::DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
            r.get(0)
        })
        .optional()?;
    value
        .map(|v| serde_json::from_str(&v).context("invalid worker registry record"))
        .transpose()
}
fn put<T: Serialize>(conn: &Connection, key: &str, value: &T) -> Result<()> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,serde_json::to_string(value)?])?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub version: String,
    pub platform: String,
    pub token_hash: String,
    pub allow_scripts: bool,
    pub reported_scripts: bool,
    pub registered_at: i64,
    pub revoked: bool,
}
#[derive(Serialize, Deserialize)]
struct Invite {
    node_id: String,
    allow_scripts: bool,
    expires_at: i64,
    used: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JoinRequest {
    pub protocol: u32,
    pub node_id: String,
    pub label: String,
    pub version: String,
    pub platform: String,
    pub token_hash: String,
    pub allow_scripts: bool,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub protocol: u32,
    pub node_id: String,
    pub controller_id: String,
    pub controller_url: String,
    pub controller_version: String,
    pub allow_scripts: bool,
}
pub fn controller_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get::<String>(conn, "worker.controller_id")? {
        Uuid::parse_str(&id)?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    put(conn, "worker.controller_id", &id)?;
    Ok(id)
}
pub fn node(conn: &Connection, id: &str) -> Result<Option<Node>> {
    ensure!(crate::remote::valid_node(id), "invalid worker ID");
    let node = get::<Node>(conn, &format!("{NODE_PREFIX}{id}"))?;
    ensure!(
        node.as_ref().is_none_or(|node| node.id == id),
        "worker registry identity mismatch"
    );
    Ok(node)
}
pub fn nodes(conn: &Connection) -> Result<Vec<Node>> {
    let mut stmt =
        conn.prepare("SELECT value FROM settings WHERE key LIKE 'worker.node.%' ORDER BY key")?;
    let values = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    values
        .into_iter()
        .map(|v| serde_json::from_str(&v).context("invalid worker registry record"))
        .collect()
}
pub fn invite(
    conn: &mut Connection,
    node_id: &str,
    allow_scripts: bool,
    ttl: i64,
) -> Result<(String, i64)> {
    ensure!(
        crate::remote::valid_node(node_id) && (60..=86400).contains(&ttl),
        "invalid node ID or expiry"
    );
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        node(&tx, node_id)?.is_none_or(|n| n.revoked),
        "node is already registered"
    );
    // Supersede previous unused invitations for this ID. Expired/used records
    // may be removed: hashes are unguessable and absence still rejects replay.
    let mut records = vec![];
    {
        let mut stmt =
            tx.prepare("SELECT key,value FROM settings WHERE key LIKE 'worker.bootstrap.%'")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (key, value) = row?;
            let old: Invite = serde_json::from_str(&value)?;
            if old.used || old.expires_at <= crate::db::now() || old.node_id == node_id {
                records.push(key);
            }
        }
    }
    for key in records {
        tx.execute("DELETE FROM settings WHERE key=?", [key])?;
    }
    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM settings WHERE key LIKE 'worker.bootstrap.%'",
        [],
        |r| r.get(0),
    )?;
    ensure!(count < 1024, "bootstrap invitation limit reached");
    let secret = secret();
    let expires_at = crate::db::now() + ttl;
    put(
        &tx,
        &format!("{INVITE_PREFIX}{}", hash(&secret)),
        &Invite {
            node_id: node_id.into(),
            allow_scripts,
            expires_at,
            used: false,
        },
    )?;
    controller_id(&tx)?;
    tx.commit()?;
    Ok((secret, expires_at))
}
pub fn redeem(
    conn: &mut Connection,
    key: &str,
    request: &JoinRequest,
    url: &str,
) -> Result<Registration> {
    ensure!(
        key.len() == 64 && request.protocol == 1 && crate::remote::valid_node(&request.node_id),
        "invalid admission request"
    );
    ensure!(
        request.token_hash.len() == 64
            && request
                .token_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid credential hash"
    );
    ensure!(
        request.label.len() <= 120
            && request.platform.len() <= 120
            && request.version.len() <= 40
            && !request.label.chars().any(char::is_control),
        "invalid worker metadata"
    );
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let storage_key = format!("{INVITE_PREFIX}{}", hash(key));
    let mut invite: Invite = get(&tx, &storage_key)?.context("bootstrap key rejected")?;
    ensure!(
        !invite.used && invite.expires_at > crate::db::now() && invite.node_id == request.node_id,
        "bootstrap key rejected"
    );
    ensure!(
        node(&tx, &request.node_id)?.is_none_or(|n| n.revoked),
        "node already registered"
    );
    ensure!(
        node(&tx, &request.node_id)?.is_none_or(|n| n.token_hash != request.token_hash),
        "revoked credentials cannot be reused"
    );
    let existing = nodes(&tx)?;
    ensure!(
        existing.len() < 1024 || existing.iter().any(|n| n.id == request.node_id),
        "worker registry limit reached"
    );
    ensure!(
        existing.iter().filter(|n| !n.revoked).count() < 128,
        "registered worker limit reached"
    );
    ensure!(
        !existing
            .iter()
            .any(|n| n.id != request.node_id && !n.revoked && n.token_hash == request.token_hash),
        "credential already belongs to another worker"
    );
    let node = Node {
        id: request.node_id.clone(),
        label: request.label.clone(),
        version: request.version.clone(),
        platform: request.platform.clone(),
        token_hash: request.token_hash.clone(),
        allow_scripts: invite.allow_scripts,
        reported_scripts: request.allow_scripts,
        registered_at: crate::db::now(),
        revoked: false,
    };
    put(&tx, &format!("{NODE_PREFIX}{}", request.node_id), &node)?;
    invite.used = true;
    put(&tx, &storage_key, &invite)?;
    let response = registration(&tx, &node, url)?;
    tx.commit()?;
    Ok(response)
}
pub fn registration(conn: &Connection, node: &Node, url: &str) -> Result<Registration> {
    Ok(Registration {
        protocol: 1,
        node_id: node.id.clone(),
        controller_id: controller_id(conn)?,
        controller_url: url.into(),
        controller_version: crate::VERSION.into(),
        allow_scripts: node.allow_scripts,
    })
}
pub fn revoke(conn: &mut Connection, id: &str) -> Result<()> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut record = node(&tx, id)?.context("registered node not found")?;
    record.revoked = true;
    put(&tx, &format!("{NODE_PREFIX}{id}"), &record)?;
    tx.execute("DELETE FROM remote_runs WHERE worker=?", [id])?;
    // Every pending invitation for this node is invalidated as part of revoke.
    let mut keys = vec![];
    {
        let mut stmt =
            tx.prepare("SELECT key,value FROM settings WHERE key LIKE 'worker.bootstrap.%'")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (key, value) = row?;
            if serde_json::from_str::<Invite>(&value)?.node_id == id {
                keys.push(key);
            }
        }
    }
    for key in keys {
        tx.execute("DELETE FROM settings WHERE key=?", [key])?;
    }
    tx.commit()?;
    Ok(())
}

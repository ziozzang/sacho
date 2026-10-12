//! Versioned, backend-independent data snapshots. SQL identifiers come only from
//! this catalog; importing a snapshot never executes supplied SQL.
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};
pub mod leases;
pub mod postgres;

pub const FORMAT_VERSION: u32 = 1;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const TABLES: &[(&str, &[&str])] = &[
    (
        "items",
        &[
            "id",
            "spec",
            "status",
            "last_check",
            "latency_ms",
            "failures",
            "successes",
            "revision",
        ],
    ),
    (
        "checks",
        &[
            "id",
            "item_id",
            "at",
            "status",
            "latency_ms",
            "output",
            "detail",
            "metric_value",
            "display_value",
        ],
    ),
    (
        "incidents",
        &[
            "id",
            "item_id",
            "name",
            "status",
            "opened_at",
            "resolved_at",
            "detail",
        ],
    ),
    (
        "subscribers",
        &["id", "email", "token", "verified", "created_at"],
    ),
    ("webhooks", &["id", "name", "url", "enabled"]),
    (
        "outbox",
        &[
            "id",
            "kind",
            "destination",
            "payload",
            "attempts",
            "next_at",
            "delivered_at",
            "error",
            "created_at",
        ],
    ),
    ("settings", &["key", "value"]),
];

const REMOTE_TABLE: (&str, &[&str]) = (
    "remote_runs",
    &[
        "item_id",
        "run_id",
        "worker",
        "revision",
        "lease_until",
        "result_hash",
    ],
);
pub(super) fn catalog(version: u32) -> Vec<(&'static str, &'static [&'static str])> {
    let mut tables = TABLES.to_vec();
    if version == 2 {
        tables.push(REMOTE_TABLE);
    }
    tables
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub format_version: u32,
    pub schema_version: u32,
    pub tables: BTreeMap<String, Vec<Vec<Value>>>,
    pub sha256: String,
}
impl Snapshot {
    fn digest(&self) -> Result<String> {
        // Sorted table names and primary-key order make this backend independent.
        let bytes = serde_json::to_vec(&(self.format_version, self.schema_version, &self.tables))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format_version == FORMAT_VERSION && (1..=2).contains(&self.schema_version),
            "unsupported snapshot version"
        );
        ensure!(
            self.tables.len() == catalog(self.schema_version).len(),
            "snapshot table catalog mismatch"
        );
        for (name, columns) in catalog(self.schema_version) {
            let rows = self.tables.get(name).context("missing snapshot table")?;
            for row in rows {
                ensure!(
                    row.len() == columns.len(),
                    "snapshot row width mismatch in {name}"
                );
                for cell in row {
                    ensure!(
                        matches!(cell, Value::Null | Value::String(_) | Value::Number(_)),
                        "unsupported snapshot cell"
                    );
                    if let Value::Number(number) = cell {
                        ensure!(
                            number.as_i64().is_some() || number.is_f64(),
                            "integer outside database range"
                        );
                    }
                }
            }
        }
        ensure!(
            self.sha256 == self.digest()?,
            "snapshot fingerprint mismatch"
        );
        Ok(())
    }
    /// Normalize a legacy seven-table snapshot into the current data catalog.
    pub fn upgraded(&self) -> Result<Self> {
        self.validate()?;
        let mut result = self.clone();
        result.schema_version = 2;
        result.tables.entry("remote_runs".into()).or_default();
        result.sha256 = result.digest()?;
        Ok(result)
    }
    pub fn counts(&self) -> BTreeMap<String, usize> {
        self.tables
            .iter()
            .map(|(name, rows)| (name.clone(), rows.len()))
            .collect()
    }
}

pub fn capture(conn: &mut Connection) -> Result<Snapshot> {
    let tx = conn.transaction()?;
    let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version == 1, "unsupported database schema");
    // Refuse silently losing future/custom persistent tables.
    let names: Vec<String> = tx.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let version = if names.iter().any(|name| name == "remote_runs") {
        2
    } else {
        1
    };
    let tables_catalog = catalog(version);
    let mut expected: Vec<String> = tables_catalog
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    expected.sort();
    ensure!(
        names == expected,
        "database table catalog mismatch; use a compatible binary"
    );
    let mut tables = BTreeMap::new();
    for (name, columns) in tables_catalog {
        let actual: Vec<String> = tx
            .prepare(&format!("PRAGMA table_info({name})"))?
            .query_map([], |r| r.get(1))?
            .collect::<rusqlite::Result<_>>()?;
        ensure!(
            actual
                .iter()
                .map(String::as_str)
                .eq(columns.iter().copied()),
            "database column catalog mismatch in {name}"
        );
        let sql = format!(
            "SELECT {} FROM {name} ORDER BY {}",
            columns.join(","),
            columns[0]
        );
        let mut statement = tx.prepare(&sql)?;
        let mut cursor = statement.query([])?;
        let mut rows = vec![];
        while let Some(row) = cursor.next()? {
            let mut cells = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                cells.push(match row.get_ref(index)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(value) => Value::from(value),
                    ValueRef::Real(value) => Value::Number(
                        serde_json::Number::from_f64(value)
                            .context("non-finite database number")?,
                    ),
                    ValueRef::Text(value) => Value::String(std::str::from_utf8(value)?.to_owned()),
                    ValueRef::Blob(_) => anyhow::bail!("unsupported blob in snapshot"),
                });
            }
            rows.push(cells);
        }
        tables.insert(name.to_string(), rows);
    }
    tx.commit()?;
    let mut snapshot = Snapshot {
        format_version: FORMAT_VERSION,
        schema_version: version,
        tables,
        sha256: String::new(),
    };
    snapshot.sha256 = snapshot.digest()?;
    Ok(snapshot)
}

pub fn load(path: &Path) -> Result<Snapshot> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_BYTES,
        "snapshot must be a regular file up to 512 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "snapshot too large");
    let snapshot: Snapshot = serde_json::from_slice(&bytes).context("invalid snapshot")?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

pub fn export_sqlite(database: &Path, destination: &Path) -> Result<Snapshot> {
    let mut conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let snapshot = capture(&mut conn)?;
    write(&snapshot, destination)?;
    Ok(snapshot)
}

pub fn write(snapshot: &Snapshot, destination: &Path) -> Result<()> {
    snapshot.validate()?;
    let mut output = tempfile::NamedTempFile::new_in(parent(destination))?;
    serde_json::to_writer(output.as_file_mut(), &snapshot)?;
    output.write_all(b"\n")?;
    ensure!(
        output.as_file().metadata()?.len() <= MAX_BYTES,
        "snapshot exceeds import limit"
    );
    output.as_file().sync_all()?;
    output.persist_noclobber(destination).map_err(|e| e.error)?;
    std::fs::File::open(parent(destination))?.sync_all()?;
    Ok(())
}

pub fn import_sqlite(snapshot: &Snapshot, destination: &Path) -> Result<()> {
    let snapshot = snapshot.upgraded()?;
    // Build and verify off to the side. Never overwrite or partially populate a
    // user's existing database, even when a constraint fails halfway through.
    let output = tempfile::NamedTempFile::new_in(parent(destination))?;
    {
        let db = crate::db::open(
            output
                .path()
                .to_str()
                .context("database path must be UTF-8")?,
        )?;
        let mut conn = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        let tx = conn.transaction()?;
        for (name, columns) in catalog(snapshot.schema_version) {
            let placeholders = vec!["?"; columns.len()].join(",");
            let sql = format!(
                "INSERT INTO {name}({}) VALUES({placeholders})",
                columns.join(",")
            );
            let mut statement = tx.prepare(&sql)?;
            for row in &snapshot.tables[name] {
                let cells: Vec<SqlValue> = row
                    .iter()
                    .map(|cell| match cell {
                        Value::Null => SqlValue::Null,
                        Value::String(value) => SqlValue::Text(value.clone()),
                        Value::Number(value) => value
                            .as_i64()
                            .map(SqlValue::Integer)
                            .unwrap_or_else(|| SqlValue::Real(value.as_f64().unwrap())),
                        _ => unreachable!("validated snapshot cell"),
                    })
                    .collect();
                statement.execute(rusqlite::params_from_iter(cells))?;
            }
        }
        tx.commit()?;
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "restored database integrity failed");
        ensure!(
            capture(&mut conn)?.sha256 == snapshot.sha256,
            "restored database fingerprint mismatch"
        );
        // Convert back to a standalone file before atomic publication.
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
    }
    output.as_file().sync_all()?;
    output.persist_noclobber(destination).map_err(|e| e.error)?;
    std::fs::File::open(parent(destination))?.sync_all()?;
    Ok(())
}

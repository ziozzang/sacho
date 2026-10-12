//! Native async PostgreSQL snapshot adapter. The application serving backend is
//! still SQLite until all repositories and ownership protocols are wired.
use super::{FORMAT_VERSION, Snapshot, catalog};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sqlx_core::row::Row;
use sqlx_postgres::{PgPool, PgPoolOptions, Postgres};
use std::time::Duration;

const SCHEMA: &str = include_str!("postgres-schema.sql");
const MIGRATION_LOCK: i64 = 0x736163686f000001;

pub async fn connect(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
        .context("PostgreSQL connection failed")
}

async fn capture_transaction(
    tx: &mut sqlx_core::transaction::Transaction<'_, Postgres>,
) -> Result<Snapshot> {
    let versions: Vec<i64> = sqlx_core::query_scalar::query_scalar(
        "SELECT version FROM sacho.schema_migrations ORDER BY version",
    )
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        versions == vec![1] || versions == vec![1, 2],
        "unsupported PostgreSQL schema"
    );
    let version = if versions.len() == 2 { 2 } else { 1 };
    let tables_catalog = catalog(version);
    let names: Vec<String> = sqlx_core::query_scalar::query_scalar(
        "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname='sacho' ORDER BY tablename",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut expected: Vec<String> = tables_catalog
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    expected.push("schema_migrations".into());
    expected.sort();
    ensure!(names == expected, "PostgreSQL table catalog mismatch");
    let mut tables = std::collections::BTreeMap::new();
    for (name, columns) in tables_catalog {
        let actual: Vec<String> = sqlx_core::query_scalar::query_scalar(
            "SELECT column_name::text FROM information_schema.columns WHERE table_schema='sacho' AND table_name=$1 ORDER BY ordinal_position",
        ).bind(name).fetch_all(&mut **tx).await?;
        ensure!(
            actual
                .iter()
                .map(String::as_str)
                .eq(columns.iter().copied()),
            "PostgreSQL column catalog mismatch in {name}"
        );
        // Force byte-wise text order, independent of database locale.
        let order = if matches!(name, "checks" | "outbox") {
            columns[0].to_string()
        } else {
            format!("{} COLLATE \"C\"", columns[0])
        };
        let query = format!("SELECT to_jsonb(t) AS data FROM sacho.{name} t ORDER BY {order}");
        let mut rows = Vec::new();
        for row in sqlx_core::query::query(&query).fetch_all(&mut **tx).await? {
            let data: Value = row.try_get("data")?;
            let mut cells = Vec::with_capacity(columns.len());
            for column in columns {
                let mut value = data
                    .get(*column)
                    .context("missing PostgreSQL column")?
                    .clone();
                if *column == "metric_value" && !value.is_null() {
                    value = Value::Number(
                        serde_json::Number::from_f64(value.as_f64().context("invalid metric")?)
                            .context("non-finite metric")?,
                    );
                }
                cells.push(value);
            }
            rows.push(cells);
        }
        tables.insert(name.to_string(), rows);
    }
    let mut snapshot = Snapshot {
        format_version: FORMAT_VERSION,
        schema_version: version,
        tables,
        sha256: String::new(),
    };
    snapshot.sha256 = snapshot.digest()?;
    snapshot.validate()?;
    Ok(snapshot)
}

pub async fn capture(pool: &PgPool) -> Result<Snapshot> {
    let mut tx = pool.begin().await?;
    sqlx_core::query::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx_core::query::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut *tx)
        .await?;
    let snapshot = capture_transaction(&mut tx).await?;
    tx.commit().await?;
    Ok(snapshot)
}

pub async fn import(snapshot: &Snapshot, pool: &PgPool) -> Result<()> {
    let snapshot = snapshot.upgraded()?;
    let mut tx = pool.begin().await?;
    sqlx_core::query::query("SET LOCAL lock_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx_core::query::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut *tx)
        .await?;
    sqlx_core::query::query("SELECT pg_advisory_xact_lock($1)")
        .bind(MIGRATION_LOCK)
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx_core::query_scalar::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_namespace WHERE nspname='sacho')",
    )
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        !exists,
        "target sacho schema already exists; import requires a new namespace"
    );
    sqlx_core::raw_sql::raw_sql(SCHEMA)
        .execute(&mut *tx)
        .await?;
    for (name, columns) in catalog(snapshot.schema_version) {
        let query = format!(
            "INSERT INTO sacho.{name}({}) SELECT {} FROM jsonb_populate_record(NULL::sacho.{name}, $1)",
            columns.join(","),
            columns.join(",")
        );
        for row in &snapshot.tables[name] {
            let data: serde_json::Map<String, Value> = columns
                .iter()
                .zip(row)
                .map(|(column, cell)| (column.to_string(), cell.clone()))
                .collect();
            sqlx_core::query::query(&query)
                .bind(Value::Object(data))
                .execute(&mut *tx)
                .await?;
        }
    }
    for table in ["checks", "outbox"] {
        let query = format!(
            "SELECT setval(pg_get_serial_sequence('sacho.{table}','id'), GREATEST(COALESCE((SELECT MAX(id) FROM sacho.{table}),1),1), EXISTS(SELECT 1 FROM sacho.{table} WHERE id>=1))"
        );
        sqlx_core::query::query(&query).execute(&mut *tx).await?;
    }
    ensure!(
        capture_transaction(&mut tx).await?.sha256 == snapshot.sha256,
        "PostgreSQL restore fingerprint mismatch"
    );
    tx.commit().await?;
    Ok(())
}

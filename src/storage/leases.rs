//! PostgreSQL coordination primitives, not an enabled distributed worker runtime.
//! Lease rows are ephemeral coordination state outside the portable business data.
use anyhow::{Result, ensure};
use sqlx_core::{query::query, row::Row, transaction::Transaction};
use sqlx_postgres::{PgPool, Postgres};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Lease {
    pub resource: String,
    pub owner: String,
    pub generation: String,
    pub token: i64,
}
fn validate(resource: &str, owner: &str, seconds: i64) -> Result<()> {
    ensure!(
        !resource.is_empty() && resource.len() <= 256 && !owner.is_empty() && owner.len() <= 256,
        "invalid lease identity"
    );
    ensure!(
        (1..=3600).contains(&seconds),
        "lease duration must be 1..3600 seconds"
    );
    Ok(())
}

pub async fn initialize(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    query("SET LOCAL lock_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    query("SELECT pg_advisory_xact_lock(8314044029760126978)")
        .execute(&mut *tx)
        .await?;
    sqlx_core::raw_sql::raw_sql(
        "CREATE SCHEMA IF NOT EXISTS sacho_coordination;
         CREATE TABLE IF NOT EXISTS sacho_coordination.leases(
          resource TEXT PRIMARY KEY, owner TEXT NOT NULL, generation TEXT NOT NULL,
          token BIGINT NOT NULL CHECK(token>0), expires_at TIMESTAMPTZ NOT NULL
         );",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn acquire(
    pool: &PgPool,
    resource: &str,
    owner: &str,
    seconds: i64,
) -> Result<Option<Lease>> {
    validate(resource, owner, seconds)?;
    // Even the same owner must renew, rather than steal its live lease. Process
    // identities should be fresh per boot; generation also fences database resets.
    let row = query(
        "INSERT INTO sacho_coordination.leases(resource,owner,generation,token,expires_at)
         VALUES($1,$2,$3,1,clock_timestamp()+$4::bigint*interval '1 second')
         ON CONFLICT(resource) DO UPDATE SET owner=EXCLUDED.owner,
           token=leases.token+1,expires_at=clock_timestamp()+$4::bigint*interval '1 second'
         WHERE leases.expires_at<=clock_timestamp()
         RETURNING generation,token",
    )
    .bind(resource)
    .bind(owner)
    .bind(Uuid::new_v4().to_string())
    .bind(seconds)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(Lease {
            resource: resource.into(),
            owner: owner.into(),
            generation: row.try_get("generation")?,
            token: row.try_get("token")?,
        })
    })
    .transpose()
}

pub async fn renew(pool: &PgPool, lease: &Lease, seconds: i64) -> Result<bool> {
    validate(&lease.resource, &lease.owner, seconds)?;
    let Some(mut tx) = fenced_transaction(pool, lease).await? else {
        return Ok(false);
    };
    query("UPDATE sacho_coordination.leases SET expires_at=clock_timestamp()+$2::bigint*interval '1 second' WHERE resource=$1")
        .bind(&lease.resource).bind(seconds).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn release(pool: &PgPool, lease: &Lease) -> Result<bool> {
    // Keep the row and token history; delete/reinsert would reset fencing.
    let Some(mut tx) = fenced_transaction(pool, lease).await? else {
        return Ok(false);
    };
    query("UPDATE sacho_coordination.leases SET expires_at=clock_timestamp() WHERE resource=$1")
        .bind(&lease.resource)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Hold the ownership row lock while committing domain mutations. Claimants
/// cannot take over until commit/rollback. Callers must keep this transaction
/// short and never perform probe execution or external delivery inside it.
/// A read-then-write check outside this transaction would permit stale commits.
pub async fn fenced_transaction<'a>(
    pool: &'a PgPool,
    lease: &Lease,
) -> Result<Option<Transaction<'a, Postgres>>> {
    let mut tx = pool.begin().await?;
    query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    query("SET LOCAL statement_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    query("SET LOCAL idle_in_transaction_session_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    let row = query(
        "SELECT token FROM sacho_coordination.leases
         WHERE resource=$1 AND owner=$2 AND generation=$3 AND token=$4 FOR UPDATE",
    )
    .bind(&lease.resource)
    .bind(&lease.owner)
    .bind(&lease.generation)
    .bind(lease.token)
    .fetch_optional(&mut *tx)
    .await?;
    if row.is_none() {
        tx.rollback().await?;
        return Ok(None);
    }
    // Check expiry after acquiring the lock: a lock wait itself can outlive the
    // lease, even when the row was eligible at the start of the SELECT.
    let live: bool = sqlx_core::query_scalar::query_scalar(
        "SELECT expires_at>clock_timestamp() FROM sacho_coordination.leases WHERE resource=$1",
    )
    .bind(&lease.resource)
    .fetch_one(&mut *tx)
    .await?;
    if !live {
        tx.rollback().await?;
        return Ok(None);
    }
    Ok(Some(tx))
}

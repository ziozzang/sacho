use anyhow::Result;
use sacho::{
    db::{self, CheckSpec, Observation},
    storage,
};
use serde_json::Value;

#[test]
fn portable_roundtrip_preserves_identity_history_and_delivery_state() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.db");
    let target = dir.path().join("target.db");
    let backup = dir.path().join("snapshot.json");
    let db = db::open(source.to_str().unwrap())?;
    {
        let mut conn = db.lock().unwrap();
        db::save(
            &mut conn,
            None,
            &CheckSpec {
                name: "Gateway".into(),
                target: "https://example.com/health".into(),
                ..Default::default()
            },
        )?;
        conn.execute("INSERT INTO subscribers VALUES('subscriber','person@example.com','opaque-token',1,100)", [])?;
        conn.execute(
            "INSERT INTO webhooks VALUES('hook','Events','https://example.com/hook',1)",
            [],
        )?;
        let item = db::items(&conn)?.remove(0);
        db::record(
            &mut conn,
            &item,
            &Observation {
                status: "outage".into(),
                latency_ms: 123,
                output: "failure".into(),
                detail: "unreachable".into(),
            },
        )?;
        conn.execute(
            "UPDATE outbox SET attempts=3,next_at=12345,error='temporary failure'",
            [],
        )?;
        conn.execute("INSERT INTO settings VALUES('site_title','Fixture')", [])?;
    }
    // Export while another connection is still open (consistent WAL snapshot).
    let before = storage::export_sqlite(&source, &backup)?;
    assert_eq!(before.counts()["outbox"], 2);
    storage::import_sqlite(&storage::load(&backup)?, &target)?;
    let after = storage::export_sqlite(&target, &dir.path().join("again.json"))?;
    assert_eq!(before.sha256, after.sha256);
    assert_eq!(before.tables, after.tables);
    assert!(storage::export_sqlite(&source, &backup).is_err());
    assert!(storage::import_sqlite(&before, &target).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backup)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&target)?.permissions().mode() & 0o777,
            0o600
        );
    }
    Ok(())
}

#[test]
fn corrupted_or_future_snapshots_never_create_target() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.db");
    db::open(source.to_str().unwrap())?;
    let snapshot = storage::export_sqlite(&source, &dir.path().join("snapshot.json"))?;
    for alteration in ["digest", "version", "table", "cell"] {
        let mut bad = snapshot.clone();
        match alteration {
            "digest" => bad.sha256 = "bad".into(),
            "version" => bad.format_version += 1,
            "table" => {
                bad.tables.insert("injected_sql".into(), vec![]);
            }
            "cell" => {
                bad.tables
                    .get_mut("settings")
                    .unwrap()
                    .push(vec![Value::Bool(true), Value::Null]);
            }
            _ => unreachable!(),
        }
        let target = dir.path().join(format!("{alteration}.db"));
        assert!(storage::import_sqlite(&bad, &target).is_err());
        assert!(!target.exists());
    }
    Ok(())
}

#[test]
fn unknown_persistent_tables_are_not_silently_discarded() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.db");
    let db = db::open(source.to_str().unwrap())?;
    db.lock()
        .unwrap()
        .execute("CREATE TABLE extra(id TEXT)", [])?;
    let backup = dir.path().join("snapshot.json");
    assert!(storage::export_sqlite(&source, &backup).is_err());
    assert!(!backup.exists());
    assert!(storage::export_sqlite(&dir.path().join("missing.db"), &backup).is_err());
    assert!(!dir.path().join("missing.db").exists());
    let extended = dir.path().join("extended.db");
    let extended_db = db::open(extended.to_str().unwrap())?;
    extended_db
        .lock()
        .unwrap()
        .execute("ALTER TABLE settings ADD COLUMN future_data TEXT", [])?;
    assert!(storage::export_sqlite(&extended, &backup).is_err());
    assert!(!backup.exists());
    Ok(())
}

#[tokio::test]
#[ignore = "run via tests/storage_docker.py against an owned Docker database"]
async fn postgres_roundtrip_and_transactional_failure() -> Result<()> {
    use sacho::storage::postgres;
    use sha2::{Digest, Sha256};
    let url = std::env::var("SACHO_TEST_DATABASE_URL")?;
    let pool = postgres::connect(&url).await?;
    let name: String = sqlx_core::query_scalar::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        name.starts_with("sacho_test_"),
        "requires an isolated fixture database"
    );
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("source.db");
    let db = db::open(path.to_str().unwrap())?;
    {
        let mut conn = db.lock().unwrap();
        let spec = CheckSpec {
            name: "Fixture".into(),
            target: "https://example.com/health".into(),
            ..Default::default()
        };
        let id = db::save(&mut conn, Some("missing".into()), &spec);
        assert!(id.is_err());
        let id = db::save(&mut conn, None, &spec)?;
        conn.execute(
            "INSERT INTO checks VALUES(99,?1,101,'operational',42,'ok','',1.0,NULL)",
            [&id],
        )?;
        // Byte-wise sorting and mixed Unicode must survive database collation.
        conn.execute(
            "INSERT INTO settings VALUES('Z','first'),('a','second'),('한글','unicode')",
            [],
        )?;
        conn.execute("INSERT INTO outbox VALUES(200,'webhook','https://example.com/hook','{}',3,200,NULL,'retry',100)", [])?;
        conn.execute(
            "INSERT INTO subscribers VALUES('person','fixture@example.com','opaque',1,100)",
            [],
        )?;
        conn.execute("INSERT INTO incidents VALUES('deleted-item',NULL,'Former service','outage',100,200,'resolved')", [])?;
    }
    let snapshot = storage::export_sqlite(&path, &dir.path().join("snapshot.json"))?;
    // Valid fingerprint, invalid FK: no schema or partial data may remain.
    let mut bad = snapshot.clone();
    bad.tables.get_mut("checks").unwrap()[0][1] = Value::String("nonexistent".into());
    bad.sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            bad.format_version,
            bad.schema_version,
            &bad.tables
        ))?)
    );
    assert!(postgres::import(&bad, &pool).await.is_err());
    let exists: bool = sqlx_core::query_scalar::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_namespace WHERE nspname='sacho')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!exists);
    postgres::import(&snapshot, &pool).await?;
    let after = postgres::capture(&pool).await?;
    assert_eq!(snapshot.sha256, after.sha256);
    assert_eq!(snapshot.tables, after.tables);
    assert!(postgres::import(&snapshot, &pool).await.is_err());
    let next_id: i64 = sqlx_core::query_scalar::query_scalar("INSERT INTO sacho.outbox(kind,destination,payload,next_at,created_at) VALUES('webhook','https://example.com','{}',100,100) RETURNING id").fetch_one(&pool).await?;
    assert!(next_id > 200);
    storage::import_sqlite(&after, &dir.path().join("returned.db"))?;
    let returned = storage::export_sqlite(
        &dir.path().join("returned.db"),
        &dir.path().join("returned.json"),
    )?;
    assert_eq!(snapshot.sha256, returned.sha256);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "run via tests/storage_docker.py against an owned Docker database"]
async fn postgres_lease_contention_failover_and_fencing() -> Result<()> {
    use sacho::storage::{leases, postgres};
    let pool = postgres::connect(&std::env::var("SACHO_TEST_DATABASE_URL")?).await?;
    let name: String = sqlx_core::query_scalar::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        name.starts_with("sacho_test_"),
        "requires an isolated fixture database"
    );
    leases::initialize(&pool).await?;
    leases::initialize(&pool).await?;
    let mut contenders = tokio::task::JoinSet::new();
    for index in 0..12 {
        let pool = pool.clone();
        contenders.spawn(async move {
            leases::acquire(&pool, "scheduler", &format!("node-{index}"), 30).await
        });
    }
    let mut winners = vec![];
    while let Some(result) = contenders.join_next().await {
        if let Some(lease) = result?? {
            winners.push(lease);
        }
    }
    assert_eq!(winners.len(), 1);
    let first = winners.remove(0);
    assert!(leases::renew(&pool, &first, 30).await?);
    assert!(
        leases::acquire(&pool, "scheduler", &first.owner, 30)
            .await?
            .is_none()
    );
    let tx = leases::fenced_transaction(&pool, &first).await?.unwrap();
    tx.commit().await?;
    // Expire through the fixture DB so tests never depend on long wall-clock sleeps.
    sqlx_core::query::query("UPDATE sacho_coordination.leases SET expires_at=clock_timestamp()-interval '1 second' WHERE resource='scheduler'").execute(&pool).await?;
    assert!(!leases::renew(&pool, &first, 30).await?);
    assert!(leases::fenced_transaction(&pool, &first).await?.is_none());
    let second = leases::acquire(&pool, "scheduler", "replacement", 30)
        .await?
        .unwrap();
    assert!(second.token > first.token);
    assert_eq!(second.generation, first.generation);
    assert!(!leases::release(&pool, &first).await?);
    assert!(leases::fenced_transaction(&pool, &first).await?.is_none());
    let mut forged = second.clone();
    forged.generation = "wrong-generation".into();
    assert!(!leases::renew(&pool, &forged, 30).await?);
    assert!(leases::fenced_transaction(&pool, &forged).await?.is_none());
    assert!(leases::release(&pool, &second).await?);
    let third = leases::acquire(&pool, "scheduler", "next", 30)
        .await?
        .unwrap();
    assert!(third.token > second.token);
    assert!(leases::acquire(&pool, "bad", "owner", 0).await.is_err());
    // A renewal queued behind a row lock must recheck expiry after the wait.
    let waiting = leases::acquire(&pool, "lock-wait", "owner", 1)
        .await?
        .unwrap();
    let held = leases::fenced_transaction(&pool, &waiting).await?.unwrap();
    let renewal_pool = pool.clone();
    let renewal_lease = waiting.clone();
    let renewal =
        tokio::spawn(async move { leases::renew(&renewal_pool, &renewal_lease, 30).await });
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    held.commit().await?;
    assert!(!renewal.await??);
    assert!(leases::fenced_transaction(&pool, &waiting).await?.is_none());
    pool.close().await;
    Ok(())
}

#[test]
fn legacy_catalog_upgrades_and_remote_receipts_survive_transfer() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("legacy.db");
    let db = db::open(source.to_str().unwrap())?;
    db.lock().unwrap().execute_batch(
        "DROP TABLE remote_runs; INSERT INTO settings VALUES('legacy','preserved');",
    )?;
    let legacy = storage::export_sqlite(&source, &dir.path().join("legacy.json"))?;
    assert_eq!(legacy.schema_version, 1);
    storage::import_sqlite(&legacy, &dir.path().join("upgraded.db"))?;
    let upgraded = storage::export_sqlite(
        &dir.path().join("upgraded.db"),
        &dir.path().join("upgraded.json"),
    )?;
    assert_eq!(upgraded.schema_version, 2);
    assert_eq!(upgraded.tables["settings"], legacy.tables["settings"]);
    assert!(upgraded.tables["remote_runs"].is_empty());
    assert_eq!(upgraded.sha256, legacy.upgraded()?.sha256);
    let remote_db = db::open(dir.path().join("remote.db").to_str().unwrap())?;
    {
        let mut conn = remote_db.lock().unwrap();
        db::save(
            &mut conn,
            None,
            &CheckSpec {
                name: "Edge".into(),
                worker: "edge".into(),
                target: "https://example.com/health".into(),
                ..Default::default()
            },
        )?;
        assert!(sacho::remote::claim(&mut conn, "edge", false)?.is_some());
    }
    let remote_snapshot = storage::export_sqlite(
        &dir.path().join("remote.db"),
        &dir.path().join("remote.json"),
    )?;
    assert_eq!(remote_snapshot.counts()["remote_runs"], 1);
    storage::import_sqlite(&remote_snapshot, &dir.path().join("remote-returned.db"))?;
    let returned = storage::export_sqlite(
        &dir.path().join("remote-returned.db"),
        &dir.path().join("remote-returned.json"),
    )?;
    assert_eq!(remote_snapshot.sha256, returned.sha256);
    Ok(())
}

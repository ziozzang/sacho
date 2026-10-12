use anyhow::Result;
use sacho::{
    db::{self, CheckSpec, Observation},
    remote::{self, Completion},
};

fn spec(worker: &str) -> CheckSpec {
    CheckSpec {
        name: "Remote fixture".into(),
        kind: "lua".into(),
        target: "return 'ok'".into(),
        worker: worker.into(),
        ..Default::default()
    }
}
fn result(assignment: &remote::Assignment) -> Completion {
    Completion {
        protocol: 1,
        run_id: assignment.run_id.clone(),
        item_id: assignment.item_id.clone(),
        observation: Observation {
            status: "outage".into(),
            latency_ms: 25,
            output: "failure".into(),
            detail: "fixture".into(),
        },
    }
}
#[test]
fn remote_results_are_fenced_and_commit_with_notifications_once() -> Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let id = db::save(&mut conn, None, &spec("edge"))?;
    conn.execute(
        "INSERT INTO webhooks VALUES('hook','Fixture','https://example.com/hook',1)",
        [],
    )?;
    assert!(remote::claim(&mut conn, "other", true)?.is_none());
    assert!(remote::claim(&mut conn, "edge", false)?.is_none());
    let first = remote::claim(&mut conn, "edge", true)?.unwrap();
    assert!(remote::claim(&mut conn, "edge", true)?.is_none());
    let completion = result(&first);
    assert!(!remote::complete(&mut conn, "other", true, &completion)?);
    conn.execute("UPDATE remote_runs SET lease_until=0", [])?;
    assert!(!remote::complete(&mut conn, "edge", true, &completion)?);
    let second = remote::claim(&mut conn, "edge", true)?.unwrap();
    assert_ne!(first.run_id, second.run_id);
    assert!(!remote::complete(&mut conn, "edge", true, &completion)?);
    let mut completion = result(&second);
    assert!(!remote::complete(&mut conn, "edge", false, &completion)?);
    conn.execute_batch("CREATE TRIGGER fail_delivery BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;
    assert!(remote::complete(&mut conn, "edge", true, &completion).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM checks", [], |r| r.get::<_, i64>(0))?,
        0
    );
    assert!(
        conn.query_row("SELECT result_hash FROM remote_runs", [], |r| r
            .get::<_, Option<String>>(0))?
            .is_none()
    );
    conn.execute_batch("DROP TRIGGER fail_delivery;")?;
    assert!(remote::complete(&mut conn, "edge", true, &completion)?);
    assert!(remote::complete(&mut conn, "edge", true, &completion)?);
    completion.observation.output = "conflicting duplicate".into();
    assert!(!remote::complete(&mut conn, "edge", true, &completion)?);
    for table in ["checks", "incidents", "outbox"] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))?,
            1
        );
    }
    conn.execute("UPDATE items SET last_check=NULL", [])?;
    let third = remote::claim(&mut conn, "edge", true)?.unwrap();
    db::save(&mut conn, Some(id.clone()), &spec("edge"))?;
    assert!(!remote::complete(&mut conn, "edge", true, &result(&third))?);
    let fourth = remote::claim(&mut conn, "edge", true)?.unwrap();
    conn.execute("DELETE FROM items WHERE id=?", [&id])?;
    assert!(!remote::complete(
        &mut conn,
        "edge",
        true,
        &result(&fourth)
    )?);
    Ok(())
}
#[test]
fn legacy_local_specs_remain_readable_without_worker_fields() -> Result<()> {
    let encoded = serde_json::to_value(CheckSpec::default())?;
    assert!(encoded.get("worker").is_none());
    let mut spec = spec("edge");
    assert!(spec.validate().is_ok());
    spec.worker = "../escape".into();
    assert!(spec.validate().is_err());
    Ok(())
}

#[test]
fn delivery_identity_survives_numeric_row_reuse() -> Result<()> {
    let db = db::open(":memory:")?;
    let conn = db.lock().unwrap();
    let payload = serde_json::json!({"event":"incident.started"});
    db::queue(&conn, "webhook", "https://example.com/hook", &payload)?;
    let (id, encoded): (i64, String) =
        conn.query_row("SELECT id,payload FROM outbox", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let first: serde_json::Value = serde_json::from_str(&encoded)?;
    conn.execute("DELETE FROM outbox", [])?;
    db::queue(&conn, "webhook", "https://example.com/hook", &payload)?;
    let (next, encoded): (i64, String) =
        conn.query_row("SELECT id,payload FROM outbox", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    assert_eq!(id, next); // SQLite can reuse the integer; transport identity must not.
    let second: serde_json::Value = serde_json::from_str(&encoded)?;
    assert_ne!(first["delivery_id"], second["delivery_id"]);
    uuid::Uuid::parse_str(second["delivery_id"].as_str().unwrap())?;
    Ok(())
}

#[test]
fn oldest_remote_monitor_gets_capacity_before_alphabetical_names() -> Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let first = db::save(
        &mut conn,
        None,
        &CheckSpec {
            name: "A".into(),
            ..spec("edge")
        },
    )?;
    let oldest = db::save(
        &mut conn,
        None,
        &CheckSpec {
            name: "Z".into(),
            ..spec("edge")
        },
    )?;
    conn.execute(
        "UPDATE items SET last_check=? WHERE id=?",
        rusqlite::params![db::now() - 100, first],
    )?;
    conn.execute(
        "UPDATE items SET last_check=? WHERE id=?",
        rusqlite::params![db::now() - 200, oldest],
    )?;
    let assignment = remote::claim(&mut conn, "edge", true)?.unwrap();
    assert_eq!(assignment.item_id, oldest);
    assert!(remote::complete(
        &mut conn,
        "edge",
        true,
        &result(&assignment)
    )?);
    assert_eq!(
        remote::claim(&mut conn, "edge", true)?.unwrap().item_id,
        first
    );
    Ok(())
}

#[test]
fn native_checks_need_no_script_permission_and_actions_commit_once() -> Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let native = CheckSpec {
        name: "Native TCP".into(),
        kind: "tcp".into(),
        target: "example.com:443".into(),
        worker: "edge".into(),
        actions: vec![sacho::actions::Action {
            kind: "lua".into(),
            target: "return true".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    db::save(&mut conn, None, &native)?;
    let assignment = remote::claim(&mut conn, "edge", false)?.unwrap();
    let completion = result(&assignment);
    assert!(remote::complete(&mut conn, "edge", false, &completion)?);
    assert!(remote::complete(&mut conn, "edge", false, &completion)?);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM outbox WHERE kind='action'", [], |r| r
            .get::<_, u32>(0))?,
        1
    );
    Ok(())
}

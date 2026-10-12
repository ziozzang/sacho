use anyhow::Result;
use sacho::{
    db,
    enrollment::{self, JoinRequest},
};
fn request(id: &str, token: &str) -> JoinRequest {
    JoinRequest {
        protocol: 1,
        node_id: id.into(),
        label: "Remote west".into(),
        version: sacho::VERSION.into(),
        platform: sacho::TARGET.into(),
        token_hash: enrollment::hash(token),
        allow_scripts: true,
    }
}
#[test]
fn invitations_are_bound_expiring_one_use_and_store_only_hashes() -> Result<()> {
    let db = db::open(":memory:")?;
    let mut conn = db.lock().unwrap();
    let (key, _) = enrollment::invite(&mut conn, "edge", false, 60)?;
    let token = enrollment::secret();
    assert!(
        enrollment::redeem(
            &mut conn,
            &key,
            &request("other", &token),
            "https://example.com"
        )
        .is_err()
    );
    let registered = enrollment::redeem(
        &mut conn,
        &key,
        &request("edge", &token),
        "https://example.com",
    )?;
    assert!(!registered.allow_scripts);
    assert!(
        enrollment::redeem(
            &mut conn,
            &key,
            &request("edge", &token),
            "https://example.com"
        )
        .is_err()
    );
    let all: Vec<String> = conn
        .prepare("SELECT value FROM settings")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    assert!(!all.join("").contains(&key));
    assert!(!all.join("").contains(&token));
    let identity = registered.controller_id;
    enrollment::revoke(&mut conn, "edge")?;
    assert!(enrollment::node(&conn, "edge")?.unwrap().revoked);
    let (expired, _) = enrollment::invite(&mut conn, "edge", true, 60)?;
    conn.execute("UPDATE settings SET value=json_set(value,'$.expires_at',0) WHERE key LIKE 'worker.bootstrap.%'",[])?;
    assert!(
        enrollment::redeem(
            &mut conn,
            &expired,
            &request("edge", &token),
            "https://example.com"
        )
        .is_err()
    );
    let (fresh, _) = enrollment::invite(&mut conn, "edge", true, 60)?;
    let next = enrollment::secret();
    assert_eq!(
        enrollment::redeem(
            &mut conn,
            &fresh,
            &request("edge", &next),
            "https://example.com"
        )?
        .controller_id,
        identity
    );
    assert_ne!(
        enrollment::node(&conn, "edge")?.unwrap().token_hash,
        enrollment::hash(&token)
    );
    Ok(())
}
#[test]
fn concurrent_redemption_has_one_winner() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("state.db");
    let db = db::open(path.to_str().unwrap())?;
    let (key, _) = enrollment::invite(&mut db.lock().unwrap(), "edge", false, 60)?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut threads = vec![];
    for _ in 0..2 {
        let path = path.clone();
        let key = key.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let db = db::open(path.to_str().unwrap()).unwrap();
            barrier.wait();
            let result = enrollment::redeem(
                &mut db.lock().unwrap(),
                &key,
                &request("edge", &enrollment::secret()),
                "https://example.com",
            );
            result.is_ok()
        }));
    }
    barrier.wait();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
    Ok(())
}

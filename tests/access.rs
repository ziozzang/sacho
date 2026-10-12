use sacho::{
    access,
    db::{self, CheckSpec, Observation},
    monitoring,
};
use serde_json::Value;
#[test]
fn queued_public_email_rechecks_current_visibility_but_operational_webhooks_remain_allowed()
-> anyhow::Result<()> {
    let database = db::open(":memory:")?;
    let mut conn = database.lock().unwrap();
    conn.execute("INSERT INTO subscribers(id,email,token,verified,created_at) VALUES('subscriber','reader@example.com','fixture-token',1,?)",[db::now()])?;
    conn.execute("INSERT INTO webhooks(id,name,url) VALUES('hook','operational fixture','http://localhost/hook')",[])?;
    let mut spec = CheckSpec {
        name: "Visibility fixture".into(),
        target: "https://example.com".into(),
        ..Default::default()
    };
    let id = db::save(&mut conn, None, &spec)?;
    let item = db::items(&conn)?.remove(0);
    db::record(
        &mut conn,
        &item,
        &Observation {
            status: "outage".into(),
            latency_ms: 1,
            output: String::new(),
            detail: String::new(),
        },
    )?;
    let (email_id, text): (i64, String) = conn.query_row(
        "SELECT id,payload FROM outbox WHERE kind='email'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let event: Value = serde_json::from_str(&text)?;
    assert!(access::public_event(&conn, &event)?);
    spec.visibility = "groups".into();
    spec.access_groups = vec!["operations".into()];
    db::save(&mut conn, Some(id), &spec)?;
    assert!(!monitoring::permit_delivery(
        &conn,
        email_id,
        "email",
        "reader@example.com",
        &event
    )?);
    assert!(monitoring::permit_delivery(
        &conn,
        999,
        "webhook",
        "http://localhost/hook",
        &event
    )?);
    let reason: String =
        conn.query_row("SELECT error FROM outbox WHERE id=?", [email_id], |r| {
            r.get(0)
        })?;
    assert_eq!(reason, "suppressed: restricted monitoring data");
    Ok(())
}

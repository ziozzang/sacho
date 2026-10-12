use chrono::{TimeZone, Utc};
use sacho::{db::CheckSpec, scheduling};
#[test]
fn cron_five_fields_timezone_and_dst_are_explicit() -> anyhow::Result<()> {
    let spec = CheckSpec {
        cron: "0 9 * * *".into(),
        schedule_timezone: "Asia/Seoul".into(),
        ..Default::default()
    };
    scheduling::validate(&spec)?;
    let now = Utc
        .with_ymd_and_hms(2026, 1, 1, 23, 59, 0)
        .unwrap()
        .timestamp();
    assert_eq!(
        scheduling::next_after(&spec, now),
        Some(
            Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0)
                .unwrap()
                .timestamp()
        )
    );
    let dst = CheckSpec {
        schedule_timezone: "America/New_York".into(),
        ..spec
    };
    let before = Utc
        .with_ymd_and_hms(2026, 3, 8, 0, 0, 0)
        .unwrap()
        .timestamp();
    assert_eq!(
        scheduling::next_after(&dst, before),
        Some(
            Utc.with_ymd_and_hms(2026, 3, 8, 13, 0, 0)
                .unwrap()
                .timestamp()
        )
    );
    assert!(
        scheduling::validate(&CheckSpec {
            cron: "broken".into(),
            ..Default::default()
        })
        .is_err()
    );
    Ok(())
}
#[test]
fn prometheus_rejects_ambiguity_missing_and_nonfinite_samples() -> anyhow::Result<()> {
    for result in [
        "[]",
        "[{\"value\":[1,\"1\"]},{\"value\":[1,\"2\"]}]",
        "[{\"value\":[1,\"NaN\"]}]",
        "[{\"value\":[1,\"+Inf\"]}]",
    ] {
        assert!(sacho::sources::prometheus(format!("{{\"status\":\"success\",\"data\":{{\"resultType\":\"vector\",\"result\":{result}}}}}").as_bytes()).is_err());
    }
    let out = sacho::sources::prometheus(
        br#"{"status":"success","data":{"resultType":"scalar","result":[1,"0"]}}"#,
    )?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out)?["value"].as_f64(),
        Some(0.0)
    );
    Ok(())
}

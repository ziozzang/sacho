//! Bounded cron scheduling in an explicit IANA timezone; no catch-up storms.
use crate::db::{CheckSpec, Item};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use cron::Schedule;
use rusqlite::{Connection, OptionalExtension};

fn schedule(spec: &CheckSpec) -> Result<Schedule> {
    let fields = spec.cron.split_whitespace().count();
    ensure!(
        fields == 5 || fields == 6 || fields == 7,
        "cron needs five fields or an optional seconds/year field"
    );
    let expression = if fields == 5 {
        format!("0 {}", spec.cron)
    } else {
        spec.cron.clone()
    };
    Ok(expression.parse()?)
}
pub fn validate(spec: &CheckSpec) -> Result<()> {
    ensure!(
        spec.cron.len() <= 256 && spec.schedule_timezone.len() <= 64,
        "schedule fields exceed limit"
    );
    ensure!(
        spec.initial_delay_seconds <= 86400,
        "initial delay exceeds one day"
    );
    let _: chrono_tz::Tz = spec.schedule_timezone.parse()?;
    if !spec.cron.is_empty() {
        let _ = schedule(spec)?;
    }
    Ok(())
}
pub fn next_after(spec: &CheckSpec, at: i64) -> Option<i64> {
    if spec.cron.is_empty() {
        return Some(at.saturating_add(spec.interval_seconds as i64));
    }
    let tz: chrono_tz::Tz = spec.schedule_timezone.parse().ok()?;
    let after = DateTime::<Utc>::from_timestamp(at, 0)?.with_timezone(&tz);
    schedule(spec)
        .ok()?
        .after(&after)
        .next()
        .map(|dt| dt.timestamp())
}
pub fn next(conn: &Connection, item: &Item) -> Result<Option<i64>> {
    let scheduled = scheduled_next(conn, item)?;
    let hold: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("schedule.hold.{}", item.id)],
            |r| r.get(0),
        )
        .optional()?;
    let hold = hold.and_then(|v| v.parse::<i64>().ok());
    Ok(scheduled.map(|at| hold.map_or(at, |until| at.max(until))))
}
fn scheduled_next(conn: &Connection, item: &Item) -> Result<Option<i64>> {
    if let Some(last) = item.last_check {
        return Ok(next_after(&item.spec, last));
    }
    let start: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("schedule.start.{}", item.id)],
            |r| r.get(0),
        )
        .optional()?;
    let start = start
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or_else(crate::db::now);
    let ready = start
        .saturating_add(item.spec.initial_delay_seconds as i64)
        .saturating_add(if item.spec.kind == "push" {
            item.spec.interval_seconds as i64
        } else {
            0
        });
    Ok(if item.spec.cron.is_empty() {
        Some(ready)
    } else {
        next_after(&item.spec, ready.saturating_sub(1))
    })
}
pub fn due(conn: &Connection, item: &Item, now: i64) -> Result<bool> {
    let hold: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("schedule.hold.{}", item.id)],
            |r| r.get(0),
        )
        .optional()?;
    if hold
        .and_then(|s| s.parse::<i64>().ok())
        .is_some_and(|until| until > now)
    {
        return Ok(false);
    }
    let forced: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [format!("schedule.run.{}", item.id)],
        |r| r.get(0),
    )?;
    if forced {
        return Ok(true);
    }
    if item.spec.cron.is_empty() {
        return Ok(next(conn, item)?.is_some_and(|at| at <= now));
    }
    // One run for the latest due slot. Missed slots during shutdown are coalesced.
    Ok(next(conn, item)?.is_some_and(|at| at <= now))
}

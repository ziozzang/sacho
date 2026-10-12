use crate::{
    config::Config,
    db::{self, Db},
};
use anyhow::{Context, Result, ensure};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::authentication::Credentials,
};
use rusqlite::params;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

pub async fn email(cfg: &Config, to: &str, subject: &str, body: &str) -> Result<()> {
    let smtp = cfg.smtp.as_ref().context("SMTP is not configured")?;
    let builder = match smtp.security.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)?,
        "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)?,
        "plain" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host),
        _ => anyhow::bail!("invalid SMTP security"),
    }
    .port(smtp.port)
    .timeout(Some(Duration::from_secs(15)));
    let builder = if let Some(username) = &smtp.username {
        let password = cfg.environment(
            smtp.password_env
                .as_deref()
                .context("SMTP password_env required with username")?,
        )?;
        builder.credentials(Credentials::new(username.clone(), password))
    } else {
        builder
    };
    let message = Message::builder()
        .from(smtp.from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .body(body.to_string())?;
    builder.build().send(message).await?;
    Ok(())
}
async fn deliver(
    cfg: &Config,
    client: &reqwest::Client,
    kind: &str,
    to: &str,
    payload: &Value,
    templates: &crate::templates::Templates,
    site_title: &str,
) -> Result<()> {
    let context = crate::templates::context(site_title, &cfg.public_url, payload);
    if kind == "webhook" {
        let response = client
            .post(to)
            .header(
                "Idempotency-Key",
                payload["delivery_id"].as_str().unwrap_or_default(),
            )
            .json(&templates.webhook(&context, payload)?)
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "webhook returned HTTP {}",
            response.status()
        );
    } else {
        let (subject, body) = templates.email(&context)?;
        email(cfg, to, &subject, &body).await?;
    }
    Ok(())
}
type DeliveryJob = (i64, String, String, String, u32);

pub async fn worker(db: Db, cfg: Arc<Config>, mut stop: watch::Receiver<bool>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {_=stop.changed()=>break,_=tick.tick()=>{}}
        let jobs = (|| -> Result<Vec<DeliveryJob>> {
            let conn = db.lock().unwrap();
            let mut stmt=conn.prepare("SELECT id,kind,destination,payload,attempts FROM outbox WHERE delivered_at IS NULL AND attempts < 8 AND next_at <= ? ORDER BY id LIMIT 16")?;
            Ok(stmt
                .query_map([db::now()], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?)
        })();
        let Ok(jobs) = jobs else {
            tracing::error!("load notification outbox failed");
            continue;
        };
        for (id, kind, to, payload, attempts) in jobs {
            let mut payload: Value = match serde_json::from_str(&payload) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let permitted = {
                let conn = db.lock().unwrap();
                crate::monitoring::permit_delivery(&conn, id, &kind, &to, &payload)
            };
            match permitted {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => {
                    tracing::error!("notification policy check failed");
                    continue;
                }
            }
            let refreshed = {
                let conn = db.lock().unwrap();
                crate::monitoring::refresh_event(&conn, &mut payload)
            };
            if refreshed.is_err() {
                tracing::error!("refresh monitoring notification failed");
                continue;
            }
            if payload["delivery_id"].as_str().is_none() {
                payload["delivery_id"] = serde_json::json!(id.to_string());
            }
            let result = tokio::select! {_=stop.changed()=>return,result=async {
                let (templates,site_title)={let conn=db.lock().unwrap();(crate::templates::load(&conn)?,db::site(&conn,&cfg)?["title"].as_str().unwrap_or(&cfg.title).to_owned())};
                deliver(&cfg,&client,&kind,&to,&payload,&templates,&site_title).await
            }=>result};
            let conn = db.lock().unwrap();
            let update = match result {
                Ok(()) => conn.execute(
                    "UPDATE outbox SET delivered_at=?,error=NULL WHERE id=?",
                    params![db::now(), id],
                ),
                Err(_) => conn.execute(
                    "UPDATE outbox SET attempts=attempts+1,next_at=?,error=? WHERE id=?",
                    params![
                        db::now() + 30 * (1i64 << attempts),
                        "Delivery failed; check SMTP/webhook settings and connectivity",
                        id
                    ],
                ),
            };
            if let Err(error) = update {
                tracing::error!(%error,"save delivery result");
            }
        }
    }
}

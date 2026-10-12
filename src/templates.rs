//! Plain-text emails and optional JSON webhooks; no loaders, secrets or host calls.
use anyhow::{Result, ensure};
use minijinja::{Environment, UndefinedBehavior};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Templates {
    pub subject: String,
    pub body: String,
    pub verification_subject: String,
    pub verification_body: String,
    pub webhook_body: Option<String>,
}
impl Default for Templates {
    fn default() -> Self {
        Self {
  subject:"{{ site_title }} · {{ event_name }} · {{ service_name }}".into(),
  body:"{{ event_name }}: {{ service_name }}\nStatus: {{ status_name }}\n{{ public_url }}\n\nUnsubscribe / 구독 해지:\n{{ unsubscribe_url }}".into(),
  verification_subject:"{{ site_title }} · Confirm subscription / 구독 확인".into(),
  verification_body:"Confirm your subscription / 구독을 확인하세요:\n{{ confirm_url }}\n\nIf you did not request this, ignore this email. / 요청하지 않았다면 무시하세요.".into(),
  webhook_body:None,
 }
    }
}
pub fn context(site_title: &str, base: &str, payload: &Value) -> Value {
    let event = payload["event"].as_str().unwrap_or("incident.started");
    let recovered = event == "incident.resolved";
    let verifying = event == "subscription.verify";
    let status = payload["status"].as_str().unwrap_or("unknown");
    let status_name = match status {
        "operational" => "Operational / 정상 운영",
        "degraded" => "Degraded / 성능 저하",
        "outage" => "Outage / 장애",
        _ => "Unknown / 확인 대기",
    };
    let at = payload["at"].as_i64().unwrap_or_else(crate::db::now);
    let base = base.trim_end_matches('/');
    json!({"site_title":site_title,"public_url":base,"service_name":payload["name"].as_str().unwrap_or("Service"),"status":status,"status_name":status_name,"event":event,"event_name":if verifying {"Confirm subscription / 구독 확인"} else if recovered {"Recovered / 복구"} else {"Incident / 장애"},"recovered":recovered,"incident_id":payload["incident_id"].as_str().unwrap_or(""),"item_id":payload["item_id"].as_str().unwrap_or(""),"delivery_id":payload["delivery_id"].as_str().unwrap_or(""),"at":at,"time":chrono::DateTime::from_timestamp(at,0).map(|t|t.to_rfc3339()).unwrap_or_default(),"group":payload["group"].as_str().unwrap_or(""),"description":payload["description"].as_str().unwrap_or(""),"display_value":payload["display_value"].as_str().unwrap_or(""),"display_label":payload["display_label"].as_str().unwrap_or(""),"metric_unit":payload["metric_unit"].as_str().unwrap_or(""),"latency_ms":payload["latency_ms"].as_u64(),"confirm_url":if verifying {format!("{base}/subscription/confirm?token={}",payload["token"].as_str().unwrap_or_default())} else {String::new()},"unsubscribe_url":if verifying {String::new()} else {format!("{base}/subscription/unsubscribe?token={}",payload["unsubscribe_token"].as_str().unwrap_or_default())}})
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + bytes.len() > 65536 {
            return Err(std::io::Error::other("template output exceeds 64 KiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn render(source: &str, ctx: &Value) -> Result<String> {
    ensure!(source.len() <= 8192, "template source exceeds 8 KiB");
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.set_fuel(Some(10000));
    environment.set_debug(false);
    let error = |e: minijinja::Error| {
        anyhow::anyhow!("Template {} at line {}", e.kind(), e.line().unwrap_or(0))
    };
    let template = environment.template_from_str(source).map_err(error)?;
    let mut output = Limited(Vec::new());
    template
        .render_captured_to(ctx, &mut output)
        .map_err(error)?;
    Ok(String::from_utf8(output.0)?)
}
impl Templates {
    pub fn email(&self, ctx: &Value) -> Result<(String, String)> {
        let verifying = ctx["event"] == "subscription.verify";
        let subject = render(
            if verifying {
                &self.verification_subject
            } else {
                &self.subject
            },
            ctx,
        )?;
        ensure!(
            !subject.trim().is_empty()
                && subject.len() <= 512
                && !subject.chars().any(char::is_control),
            "email subject must be nonempty, single-line and at most 512 bytes"
        );
        let mut body = render(
            if verifying {
                &self.verification_body
            } else {
                &self.body
            },
            ctx,
        )?;
        let url = ctx[if verifying {
            "confirm_url"
        } else {
            "unsubscribe_url"
        }]
        .as_str()
        .unwrap_or_default();
        if !url.is_empty() && !body.contains(url) {
            body.push_str(if verifying {
                "\n\nConfirm subscription / 구독 확인:\n"
            } else {
                "\n\nUnsubscribe / 구독 해지:\n"
            });
            body.push_str(url);
        }
        ensure!(
            body.len() <= 65536 && !body.contains('\0'),
            "email body exceeds limit or contains NUL"
        );
        Ok((subject, body))
    }
    pub fn webhook(&self, ctx: &Value, payload: &Value) -> Result<Value> {
        if let Some(source) = &self.webhook_body {
            let result: Value = serde_json::from_str(&render(source, ctx)?)
                .map_err(|_| anyhow::anyhow!("webhook template must render valid JSON"))?;
            ensure!(
                result.is_object(),
                "webhook template must render a JSON object"
            );
            Ok(result)
        } else {
            Ok(payload.clone())
        }
    }
    pub fn validate(&self) -> Result<()> {
        for event in [
            "incident.started",
            "incident.resolved",
            "subscription.verify",
        ] {
            let payload = json!({"event":event,"name":"Example service","status":if event=="incident.resolved" {"operational"} else {"outage"},"incident_id":"example-incident","item_id":"example-item","delivery_id":"example-delivery","token":"example","unsubscribe_token":"example","at":1700000000,"group":"Example","description":"Example description","display_value":"42.5","display_label":"TPS","metric_unit":" TPS","latency_ms":25});
            let ctx = context("Sacho", "https://status.example.com", &payload);
            self.email(&ctx)?;
            if event != "subscription.verify" {
                self.webhook(&ctx, &payload)?;
            }
        }
        Ok(())
    }
}
pub fn load(connection: &Connection) -> Result<Templates> {
    let raw: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key='notification_templates'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match raw {
        Some(raw) => Ok(serde_json::from_str(&raw)?),
        None => Ok(Templates::default()),
    }
}

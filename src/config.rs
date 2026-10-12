use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::Path};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bind: String,
    pub database: String,
    pub public_url: String,
    pub title: String,
    pub description: String,
    pub auth: Auth,
    pub smtp: Option<Smtp>,
    pub updates: Updates,
    pub retention_days: u32,
    pub max_concurrent_checks: usize,
    pub probe_env: Vec<String>,
    pub secrets: Option<Secrets>,
    #[serde(skip)]
    pub secret_values: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Secrets {
    pub vault: String,
    pub key_file: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    pub mode: String,
    pub email_header: String,
    pub admins: Vec<String>,
    pub logout_url: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Smtp {
    pub host: String,
    #[serde(default = "smtp_port")]
    pub port: u16,
    pub from: String,
    pub username: Option<String>,
    pub password_env: Option<String>,
    #[serde(default = "smtp_security")]
    pub security: String,
}
fn smtp_port() -> u16 {
    587
}
fn smtp_security() -> String {
    "starttls".into()
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Updates {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub public_key: Option<String>,
    pub manifest_url: Option<String>,
    pub ca_file: Option<String>,
}
impl Default for Auth {
    fn default() -> Self {
        Self {
            mode: "no-auth".into(),
            email_header: "X-Forwarded-Email".into(),
            admins: vec![],
            logout_url: None,
        }
    }
}
impl Default for Updates {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: 21600,
            public_key: None,
            manifest_url: None,
            ca_file: None,
        }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            database: "sacho.db".into(),
            public_url: "http://127.0.0.1:8080".into(),
            title: "Sacho".into(),
            description: "Service health, recorded with care.".into(),
            auth: Auth::default(),
            smtp: None,
            updates: Updates::default(),
            retention_days: 100,
            max_concurrent_checks: 16,
            probe_env: vec![],
            secrets: None,
            secret_values: Default::default(),
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut cfg: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        if let Some(secrets) = &cfg.secrets {
            cfg.secret_values =
                crate::secrets::load(Path::new(&secrets.vault), Path::new(&secrets.key_file))?;
        }
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn environment(&self, name: &str) -> Result<String> {
        std::env::var(name)
            .ok()
            .or_else(|| self.secret_values.get(name).cloned())
            .ok_or_else(|| anyhow::anyhow!("named input is not available"))
    }
    pub fn validate(&self) -> Result<()> {
        let bind: SocketAddr = self.bind.parse()?;
        ensure!(
            self.auth.mode == "no-auth" || self.auth.mode == "auth-proxy",
            "auth mode must be no-auth or auth-proxy"
        );
        if let Some(url) = &self.auth.logout_url {
            ensure!(
                (url.starts_with('/') && !url.starts_with("//") && !url.contains('\\'))
                    || reqwest::Url::parse(url)
                        .is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some()),
                "logout_url must be a local absolute path or HTTPS URL"
            );
        }
        if self.auth.mode == "no-auth" {
            ensure!(
                bind.ip().is_loopback(),
                "no-auth must bind to loopback; use auth-proxy for a public listener"
            );
        } else {
            ensure!(
                !self.auth.admins.is_empty(),
                "auth-proxy requires admin email addresses"
            );
            axum::http::HeaderName::from_bytes(self.auth.email_header.as_bytes())?;
        }
        ensure!(
            self.probe_env.len() <= 32
                && self
                    .probe_env
                    .iter()
                    .all(|name| !name.starts_with("SACHO_PROBE_")
                        && name != "PATH"
                        && name != "LANG"
                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        && !name.is_empty()),
            "invalid probe_env allowlist"
        );
        let url = reqwest::Url::parse(&self.public_url)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "public_url must be an HTTP(S) origin"
        );
        ensure!(
            (90..=3650).contains(&self.retention_days),
            "retention_days must be 90..3650"
        );
        ensure!(
            (1..=128).contains(&self.max_concurrent_checks),
            "max_concurrent_checks must be 1..128"
        );
        ensure!(
            self.updates.interval_seconds >= 60,
            "update interval must be at least 60 seconds"
        );
        if let Some(smtp) = &self.smtp {
            ensure!(
                matches!(smtp.security.as_str(), "starttls" | "tls" | "plain"),
                "SMTP security must be starttls, tls or plain"
            );
            if smtp.security == "plain" {
                ensure!(
                    smtp.host == "localhost"
                        || smtp
                            .host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|ip| ip.is_loopback()
                                || matches!(ip, std::net::IpAddr::V4(v4) if v4.is_private())),
                    "plain SMTP is only allowed on loopback or private IPv4 networks"
                );
            }
            smtp.from.parse::<lettre::message::Mailbox>()?;
            if let Some(name) = &smtp.password_env {
                ensure!(
                    self.environment(name).is_ok(),
                    "SMTP password environment variable is missing"
                );
            }
        }
        Ok(())
    }
}

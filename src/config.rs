use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::Path};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bind: String,
    pub database: String,
    pub public_url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub public_origins: Vec<String>,
    pub title: String,
    pub description: String,
    pub auth: Auth,
    pub smtp: Option<Smtp>,
    pub updates: Updates,
    pub retention_days: u32,
    pub max_concurrent_checks: usize,
    pub probe_env: Vec<String>,
    pub remote_workers: Vec<RemoteWorker>,
    pub secrets: Option<Secrets>,
    #[serde(skip)]
    pub blocked_input_names: Vec<String>,
    #[serde(skip)]
    pub runtime_literals:
        std::sync::Arc<std::sync::RwLock<std::collections::BTreeMap<String, String>>>,
    #[serde(skip)]
    pub probe_overrides: std::collections::BTreeMap<String, String>,
    #[serde(skip)]
    pub runtime_inputs: std::sync::Arc<std::sync::RwLock<Vec<String>>>,
    #[serde(skip)]
    pub secret_values: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteWorker {
    pub id: String,
    pub token_env: String,
    #[serde(default)]
    pub allow_scripts: bool,
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
            public_origins: vec![],
            title: "Sacho".into(),
            description: "Service health, recorded with care.".into(),
            auth: Auth::default(),
            smtp: None,
            updates: Updates::default(),
            retention_days: 100,
            max_concurrent_checks: 16,
            probe_env: vec![],
            remote_workers: vec![],
            secrets: None,
            secret_values: Default::default(),
            runtime_inputs: Default::default(),
            probe_overrides: Default::default(),
            runtime_literals: Default::default(),
            blocked_input_names: vec![],
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
    pub fn probe_names(&self) -> Vec<String> {
        let mut names = self.probe_env.clone();
        for name in self.runtime_inputs.read().unwrap().iter() {
            if !names.contains(name) && self.manageable_input(name) {
                names.push(name.clone());
            }
        }
        names
    }
    pub fn manageable_input(&self, name: &str) -> bool {
        crate::secrets::valid_name(name)
            && !name.starts_with("SACHO_ITEM_")
            && !self.blocked_input_names.iter().any(|n| n == name)
            && !self.remote_workers.iter().any(|w| w.token_env == name)
            && self.smtp.as_ref().and_then(|s| s.password_env.as_deref()) != Some(name)
    }
    pub fn environment(&self, name: &str) -> Result<String> {
        if let Some(value) = self.probe_overrides.get(name) {
            return Ok(value.clone());
        }
        if let Ok(value) = std::env::var(name) {
            return Ok(value);
        }
        if let Some(value) = self.runtime_literals.read().unwrap().get(name) {
            return Ok(value.clone());
        }
        let values = match &self.secrets {
            Some(paths) => {
                crate::secrets::load(Path::new(&paths.vault), Path::new(&paths.key_file))?
            }
            None => self.secret_values.clone(),
        };
        values
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("named input is not available"))
    }
    pub fn validate(&self) -> Result<()> {
        let mut nodes = std::collections::HashSet::new();
        let mut tokens = std::collections::HashSet::new();
        ensure!(self.remote_workers.len() <= 128, "too many workers");
        for worker in &self.remote_workers {
            ensure!(
                crate::remote::valid_node(&worker.id) && nodes.insert(&worker.id),
                "invalid or duplicate worker ID"
            );
            let token = zeroize::Zeroizing::new(self.environment(&worker.token_env)?);
            ensure!(
                tokens.insert(<sha2::Sha256 as sha2::Digest>::digest(token.as_bytes())),
                "worker tokens must be unique per node"
            );
            ensure!(
                (32..=512).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic()),
                "worker token must be 32..512 printable bytes"
            );
            ensure!(
                !self.probe_env.contains(&worker.token_env),
                "worker admission tokens must not be exposed to probes"
            );
        }
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
        ensure!(self.public_origins.len() <= 16, "too many public origins");
        let mut origins = std::collections::HashSet::new();
        for origin in &self.public_origins {
            let url = reqwest::Url::parse(origin)?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some_and(|host| !host.contains('*'))
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                    && url.path() == "/"
                    && url.origin().ascii_serialization() == origin.trim_end_matches('/'),
                "public_origins must contain normalized HTTP(S) origins"
            );
            ensure!(
                origins.insert(origin.trim_end_matches('/')),
                "duplicate public origin"
            );
        }
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

impl Config {
    pub fn allows_origin(&self, origin: &str) -> bool {
        origin == self.public_url.trim_end_matches('/')
            || self
                .public_origins
                .iter()
                .any(|allowed| origin == allowed.trim_end_matches('/'))
    }
}

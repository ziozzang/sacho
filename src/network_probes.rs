//! Native ICMP and read-only SNMP. No shell, Lua, ping or snmpget subprocesses.
use crate::{config::Config, db::CheckSpec};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ping {
    pub count: u16,
    pub min_replies: u16,
    pub wait_ms: u64,
    pub interval_ms: u64,
}
impl Default for Ping {
    fn default() -> Self {
        Self {
            count: 3,
            min_replies: 1,
            wait_ms: 1000,
            interval_ms: 100,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Snmp {
    pub version: String,
    pub oid: String,
    pub community_env: String,
    pub username: String,
    pub security: String,
    pub auth_protocol: String,
    pub auth_env: String,
    pub privacy_env: String,
}
impl Default for Snmp {
    fn default() -> Self {
        Self {
            version: "2c".into(),
            oid: "1.3.6.1.2.1.1.3.0".into(),
            community_env: "SNMP_COMMUNITY".into(),
            username: String::new(),
            security: "authPriv".into(),
            auth_protocol: "sha256".into(),
            auth_env: "SNMP_AUTH_PASSWORD".into(),
            privacy_env: "SNMP_PRIVACY_PASSWORD".into(),
        }
    }
}
pub fn validate(spec: &CheckSpec) -> Result<()> {
    if spec.kind == "ping" {
        ensure!(
            !spec.target.trim().is_empty() && !spec.target.contains(['/', ':'])
                || spec.target.parse::<std::net::Ipv6Addr>().is_ok(),
            "ping target must be a hostname or IP without port"
        );
        let p = spec.ping.clone().unwrap_or_default();
        ensure!(
            (1..=10).contains(&p.count)
                && (1..=p.count).contains(&p.min_replies)
                && (100..=10000).contains(&p.wait_ms)
                && (100..=5000).contains(&p.interval_ms),
            "invalid ping count or timing"
        );
        ensure!(
            u64::from(p.count) * p.wait_ms + u64::from(p.count - 1) * p.interval_ms
                <= spec.timeout_seconds * 1000,
            "ping attempts exceed the check timeout"
        );
    }
    if spec.kind == "snmp" {
        let target: reqwest::Url = format!("udp://{}", spec.target).parse()?;
        ensure!(
            target.host_str().is_some()
                && target.port().is_some()
                && target.path().is_empty()
                && target.username().is_empty()
                && target.password().is_none()
                && target.query().is_none()
                && target.fragment().is_none(),
            "SNMP target needs host:port or [IPv6]:port"
        );
        let s = spec
            .snmp
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SNMP configuration required"))?;
        let _: snmp2::Oid = s
            .oid
            .parse()
            .map_err(|_| anyhow::anyhow!("SNMP needs a numeric OID"))?;
        ensure!(
            s.oid.len() <= 512
                && s.username.len() <= 64
                && matches!(s.version.as_str(), "2c" | "3")
                && matches!(
                    s.security.as_str(),
                    "noAuthNoPriv" | "authNoPriv" | "authPriv"
                )
                && matches!(s.auth_protocol.as_str(), "sha1" | "sha256"),
            "invalid SNMP options"
        );
        ensure!(
            s.version != "3" || !s.username.is_empty(),
            "SNMPv3 username required"
        );
        for name in inputs(s) {
            ensure!(
                crate::secrets::valid_name(name),
                "invalid SNMP credential input name"
            );
        }
    }
    Ok(())
}
pub fn inputs(s: &Snmp) -> Vec<&str> {
    if s.version == "2c" {
        vec![&s.community_env]
    } else {
        match s.security.as_str() {
            "noAuthNoPriv" => vec![],
            "authNoPriv" => vec![&s.auth_env],
            _ => vec![&s.auth_env, &s.privacy_env],
        }
    }
}
pub async fn ping(spec: &CheckSpec) -> Result<String> {
    let ip = if let Ok(ip) = spec.target.parse::<std::net::IpAddr>() {
        ip
    } else {
        tokio::net::lookup_host((spec.target.as_str(), 0))
            .await?
            .next()
            .ok_or_else(|| anyhow::anyhow!("ping hostname has no address"))?
            .ip()
    };
    let p = spec.ping.clone().unwrap_or_default();
    let config = surge_ping::Config::builder()
        .kind(if ip.is_ipv4() {
            surge_ping::ICMP::V4
        } else {
            surge_ping::ICMP::V6
        })
        .build();
    let client = surge_ping::Client::new(&config).map_err(|_| {
        anyhow::anyhow!(
            "ICMP socket unavailable; permit ping sockets or CAP_NET_RAW on the executing node"
        )
    })?;
    let mut pinger = client
        .pinger(
            ip,
            surge_ping::PingIdentifier((uuid::Uuid::new_v4().as_u128() & 65535) as u16),
        )
        .await;
    pinger.timeout(Duration::from_millis(p.wait_ms));
    let mut rtts = vec![];
    for seq in 0..p.count {
        if let Ok((_, duration)) = pinger
            .ping(surge_ping::PingSequence(seq), b"sacho-icmp-probe")
            .await
        {
            rtts.push(duration.as_secs_f64() * 1000.0);
        }
        if seq + 1 < p.count {
            tokio::time::sleep(Duration::from_millis(p.interval_ms)).await;
        }
    }
    let received = rtts.len();
    let mean = if received > 0 {
        Some(rtts.iter().sum::<f64>() / received as f64)
    } else {
        None
    };
    let loss = 100.0 * (1.0 - received as f64 / f64::from(p.count));
    Ok(json!({"healthy":if received>=usize::from(p.min_replies){1}else{0},"rtt_ms":mean,"rtt_min_ms":rtts.iter().copied().reduce(f64::min),"rtt_max_ms":rtts.iter().copied().reduce(f64::max),"loss_percent":loss,"sent":p.count,"received":received,"message":format!("{received}/{} ICMP replies · {loss:.0}% packet loss",p.count)}).to_string())
}
pub async fn snmp(spec: &CheckSpec, cfg: &Config) -> Result<String> {
    let s = spec
        .snmp
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("SNMP configuration required"))?;
    let input = |name: &str| -> Result<String> {
        ensure!(
            cfg.probe_names().iter().any(|n| n == name),
            "SNMP credential input is not allowed"
        );
        cfg.environment(name)
    };
    let mut session = if s.version == "2c" {
        let community = input(&s.community_env)?;
        snmp2::AsyncSession::new_v2c(&spec.target, community.as_bytes(), 0).await?
    } else {
        let password = if s.security == "noAuthNoPriv" {
            String::new()
        } else {
            input(&s.auth_env)?
        };
        ensure!(
            s.security == "noAuthNoPriv" || password.len() >= 8,
            "SNMPv3 authentication password must be at least eight bytes"
        );
        let privacy = if s.security == "authPriv" {
            let value = input(&s.privacy_env)?;
            ensure!(
                value.len() >= 8,
                "SNMPv3 privacy password must be at least eight bytes"
            );
            value.into_bytes()
        } else {
            vec![]
        };
        let security = snmp2::v3::Security::new(s.username.as_bytes(), password.as_bytes())
            .with_auth_protocol(if s.auth_protocol == "sha1" {
                snmp2::v3::AuthProtocol::Sha1
            } else {
                snmp2::v3::AuthProtocol::Sha256
            })
            .with_auth(match s.security.as_str() {
                "noAuthNoPriv" => snmp2::v3::Auth::NoAuthNoPriv,
                "authNoPriv" => snmp2::v3::Auth::AuthNoPriv,
                _ => snmp2::v3::Auth::AuthPriv {
                    cipher: snmp2::v3::Cipher::Aes128,
                    privacy_password: privacy,
                },
            });
        let mut session = snmp2::AsyncSession::new_v3(&spec.target, 0, security).await?;
        session.init().await?;
        session
    };
    let oid: snmp2::Oid = s
        .oid
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid SNMP OID"))?;
    for _ in 0..3 {
        let mut response = match session.get(&oid).await {
            Ok(r) => r,
            Err(snmp2::Error::AuthUpdated) => continue,
            Err(e) => return Err(e.into()),
        };
        ensure!(response.error_status == 0, "SNMP agent returned an error");
        let (returned, value) = response
            .varbinds
            .next()
            .ok_or_else(|| anyhow::anyhow!("SNMP response has no value"))?;
        ensure!(returned == oid, "SNMP response OID differs from request");
        let (value, kind) = match value {
            snmp2::Value::Integer(n) => (json!(n), "integer"),
            snmp2::Value::Counter32(n) => (json!(n), "counter32"),
            snmp2::Value::Counter64(n) => (json!(n), "counter64"),
            snmp2::Value::Unsigned32(n) => (json!(n), "gauge32"),
            snmp2::Value::Timeticks(n) => (json!(n), "timeticks"),
            snmp2::Value::Boolean(n) => (json!(n), "boolean"),
            snmp2::Value::OctetString(bytes) => {
                ensure!(bytes.len() <= 16384, "SNMP string exceeds limit");
                (json!(String::from_utf8_lossy(bytes)), "string")
            }
            snmp2::Value::IpAddress(ip) => (json!(std::net::Ipv4Addr::from(ip).to_string()), "ip"),
            snmp2::Value::ObjectIdentifier(oid) => (json!(oid.to_string()), "oid"),
            _ => anyhow::bail!("SNMP OID is absent or does not return a supported scalar"),
        };
        return Ok(json!({"value":value,"type":kind,"oid":s.oid}).to_string());
    }
    anyhow::bail!("SNMPv3 engine synchronization did not settle")
}

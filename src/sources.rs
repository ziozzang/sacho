//! Native monitoring adapters; provider-specific business logic stays external.
use crate::db::CheckSpec;
use anyhow::{Result, ensure};
use hickory_resolver::{
    Resolver,
    config::{NameServerConfig, ResolverConfig},
    net::runtime::TokioRuntimeProvider,
    proto::rr::{Name, RecordType},
};
use serde_json::{Value, json};

pub fn validate(spec: &CheckSpec) -> Result<()> {
    ensure!(spec.prometheus_query.len() <= 8192, "PromQL exceeds limit");
    if spec.kind == "prometheus" {
        ensure!(!spec.prometheus_query.trim().is_empty(), "PromQL required");
    }
    if spec.kind == "tcp" {
        validate_tcp(&spec.target)?;
    }
    if spec.kind == "dns" {
        let _: Name = spec.target.parse()?;
        ensure!(
            matches!(
                spec.dns_record_type.as_str(),
                "A" | "AAAA" | "CNAME" | "TXT" | "MX" | "NS" | "SOA" | "SRV" | "PTR" | "CAA"
            ),
            "unsupported DNS record type"
        );
        if !spec.dns_server.is_empty() {
            let _ = dns_address(&spec.dns_server)?;
        }
        ensure!(
            spec.dns_expected.len() <= 64 && spec.dns_expected.iter().all(|s| s.len() <= 1024),
            "DNS expected values exceed limit"
        );
    }
    if spec.kind == "push" {
        ensure!(
            spec.cron.is_empty() && spec.worker.is_empty(),
            "Push reporting uses interval expiration on the controller"
        );
    }
    Ok(())
}
pub fn prometheus(body: &[u8]) -> Result<String> {
    let value: Value = serde_json::from_slice(body)?;
    ensure!(value["status"] == "success", "Prometheus query failed");
    let data = &value["data"];
    let sample = match data["resultType"].as_str() {
        Some("scalar") => &data["result"],
        Some("vector") => {
            let entries = data["result"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid Prometheus vector"))?;
            ensure!(
                entries.len() == 1,
                "PromQL must return exactly one series; aggregate multiple series"
            );
            &entries[0]["value"]
        }
        _ => anyhow::bail!("PromQL must return an instant scalar or one series"),
    };
    let number: f64 = sample[1]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing Prometheus value"))?
        .parse()?;
    ensure!(number.is_finite(), "Prometheus sample must be finite");
    Ok(json!({"value":number,"sample_at":sample[0]}).to_string())
}
pub async fn dns(spec: &CheckSpec) -> Result<String> {
    let builder = if spec.dns_server.is_empty() {
        Resolver::builder_tokio()?
    } else {
        let address = dns_address(&spec.dns_server)?;
        let mut server = NameServerConfig::udp_and_tcp(address.ip());
        for connection in &mut server.connections {
            connection.port = address.port();
        }
        Resolver::builder_with_config(
            ResolverConfig::from_name_servers(vec![server]),
            TokioRuntimeProvider::default(),
        )
    };
    let mut builder = builder;
    builder.options_mut().cache_size = 0;
    builder.options_mut().use_hosts_file = hickory_resolver::config::ResolveHosts::Never;
    let resolver = builder.build()?;
    let result = resolver
        .lookup(
            format!("{}.", spec.target.trim_end_matches('.')),
            spec.dns_record_type.parse::<RecordType>()?,
        )
        .await?;
    let mut answers: Vec<String> = result
        .answers()
        .iter()
        .map(|r| r.data.to_string())
        .collect();
    answers.sort();
    answers.dedup();
    ensure!(
        !answers.is_empty() && answers.len() <= 256,
        "DNS answer missing or exceeds limit"
    );
    let mut expected = spec.dns_expected.clone();
    expected.sort();
    expected.dedup();
    let healthy = expected.is_empty() || answers == expected;
    Ok(json!({"healthy":if healthy{1}else{0},"display":answers.join("; "),"answers":answers,"count":answers.len()}).to_string())
}

fn validate_tcp(target: &str) -> Result<reqwest::Url> {
    let url: reqwest::Url = format!("tcp://{target}").parse()?;
    ensure!(
        url.host_str().is_some()
            && url.port().is_some()
            && url.path().is_empty()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "TCP target needs host:port or [IPv6]:port"
    );
    Ok(url)
}
pub async fn tcp(target: &str) -> Result<String> {
    let url = validate_tcp(target)?;
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    let port = url.port().unwrap();
    let start = std::time::Instant::now();
    let _socket = tokio::net::TcpStream::connect((host, port)).await?;
    Ok(json!({"healthy":1,"value":start.elapsed().as_secs_f64()*1000.0}).to_string())
}

fn dns_address(value: &str) -> Result<std::net::SocketAddr> {
    if let Ok(ip) = value.parse::<std::net::IpAddr>() {
        Ok(std::net::SocketAddr::new(ip, 53))
    } else {
        let address: std::net::SocketAddr = value.parse()?;
        ensure!(address.port() > 0, "DNS port required");
        Ok(address)
    }
}

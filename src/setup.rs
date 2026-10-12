//! Admin-only setup catalogue and portable metadata without executable sources.
use crate::{
    config::Config,
    db::{self, CheckSpec},
};
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn inputs(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    for part in source.split("sacho.env(").skip(1) {
        let part = part.trim_start();
        let Some(quote) = part.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        if let Some(end) = part[1..].find(quote) {
            let name = &part[1..1 + end];
            if name.len() <= 128
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !result.iter().any(|v| v == name)
            {
                result.push(name.to_string());
            }
        }
    }
    result
}
pub fn validate(spec: &CheckSpec, cfg: &Config) -> Value {
    let mut errors = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();
    if let Err(e) = spec.validate() {
        errors.push(e.to_string());
    }
    if spec.kind == "lua" && crate::engine::validate_lua(&spec.target).is_err() {
        errors.push("Lua syntax is invalid".into());
    }
    let mut names = spec.required_env.clone();
    for name in inputs(&spec.target) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    for name in spec.headers_env.values() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    if spec.worker.is_empty() {
        for name in &names {
            if !cfg.probe_names().contains(name) {
                warnings.push(format!("{name}: not in probe_env"));
            } else if cfg.environment(name).is_err() {
                warnings.push(format!("{name}: input is not available"));
            }
        }
    } else {
        warnings.push(
            "Remote inputs are validated on the selected agent when the saved check runs".into(),
        );
    }
    json!({"valid":errors.is_empty(),"errors":errors,"warnings":warnings,"required_env":names})
}
pub fn catalogue() -> Value {
    let http = CheckSpec {
        name: "HTTP availability".into(),
        target: "https://example.com/health".into(),
        ..Default::default()
    };
    let scalar = CheckSpec {
        name: "JSON scalar".into(),
        kind: "lua".into(),
        target:
            "local r=sacho.http(sacho.env(\"CHECK_URL\")); assert(r.status==200); return r.body"
                .into(),
        publish_value: true,
        display_pointer: "/value".into(),
        publish_trend: true,
        trend_pointer: "/value".into(),
        ..Default::default()
    };
    let prometheus = CheckSpec {
        name: "Prometheus query".into(),
        kind: "lua".into(),
        target: include_str!("../examples/lua/prometheus.lua").into(),
        publish_value: true,
        display_pointer: "/value".into(),
        publish_trend: true,
        trend_pointer: "/value".into(),
        ..Default::default()
    };
    let postgres = CheckSpec {
        name: "PostgreSQL query".into(),
        target: include_str!("../examples/lua/postgresql.lua").into(),
        ..prometheus.clone()
    };
    let docker = CheckSpec {
        name: "Docker container".into(),
        kind: "lua".into(),
        target: include_str!("../examples/lua/docker.lua").into(),
        publish_value: true,
        display_pointer: "/display".into(),
        public_message_pointer: "/message".into(),
        json_pointer: "/healthy".into(),
        threshold: Some(1.0),
        comparison: "gte".into(),
        ..Default::default()
    };
    let native_prometheus = CheckSpec {
        name: "Prometheus query".into(),
        kind: "prometheus".into(),
        target: "https://prometheus.example.com".into(),
        prometheus_query: "sum(rate(http_requests_total[5m]))".into(),
        publish_value: true,
        display_pointer: "/value".into(),
        publish_trend: true,
        trend_pointer: "/value".into(),
        ..Default::default()
    };
    let native_dns = CheckSpec {
        name: "DNS records".into(),
        kind: "dns".into(),
        target: "example.com".into(),
        publish_value: true,
        display_pointer: "/display".into(),
        json_pointer: "/healthy".into(),
        threshold: Some(1.0),
        comparison: "gte".into(),
        ..Default::default()
    };
    let native_tcp = CheckSpec {
        name: "TCP connectivity".into(),
        kind: "tcp".into(),
        target: "example.com:443".into(),
        publish_value: true,
        display_pointer: "/value".into(),
        metric_unit: "ms".into(),
        publish_trend: true,
        trend_pointer: "/value".into(),
        ..Default::default()
    };
    let push = CheckSpec {
        name: "External report".into(),
        kind: "push".into(),
        target: "push".into(),
        interval_seconds: 300,
        publish_value: true,
        display_pointer: "/value".into(),
        ..Default::default()
    };
    let starter = |name: &str, kind: &str, source: &str| CheckSpec {
        name: name.into(),
        kind: kind.into(),
        target: source.into(),
        publish_value: true,
        display_pointer: "/display".into(),
        json_pointer: "/healthy".into(),
        threshold: Some(1.0),
        comparison: "gte".into(),
        ..Default::default()
    };
    let ping = starter(
        "ICMP reachability",
        "lua",
        include_str!("../examples/lua/ping.lua"),
    );
    let mut examples = json!([
        {"id":"ping","title":"ICMP ping","spec":ping,"inputs":["PING_HOST"],"requirements":"External ping with ICMP permission; firewalls may block ICMP independently of service health"},
        {"id":"prometheus-native","title":"Prometheus (native)","spec":native_prometheus,"inputs":[],"requirements":"Prometheus instant-query API; exactly one scalar or series"},
        {"id":"dns","title":"DNS records","spec":native_dns,"inputs":[],"requirements":"Built-in resolver; optional DNS server IP and exact expected answer set"},
        {"id":"tcp","title":"TCP connection time","spec":native_tcp,"inputs":[],"requirements":"Built in; host:port or [IPv6]:port"},
        {"id":"push","title":"External report / heartbeat","spec":push,"inputs":[],"requirements":"Save, issue a reporting token, then POST status and data within the configured interval"},
        {"id":"shell","title":"Shell script","spec":starter("Shell clock", "shell", include_str!("../examples/shell/basic.sh")),"inputs":[],"requirements":"/bin/sh and date on the executing node"},
        {"id":"python","title":"Python code","spec":starter("Python version", "python", include_str!("../examples/python/basic.py")),"inputs":[],"requirements":"python3 on the executing node"},
        {"id":"lua","title":"Lua script","spec":starter("Embedded Lua version", "lua", include_str!("../examples/lua/basic.lua")),"inputs":[],"requirements":"Built in; no external interpreter or inputs required"},
        {"id":"http","title":"HTTP / API","spec":http,"inputs":[],"requirements":"HTTP(S) connectivity"},
        {"id":"json","title":"JSON scalar / TPS","spec":scalar,"inputs":["CHECK_URL"],"requirements":"A JSON response with a numeric value field"},
        {"id":"prometheus","title":"Prometheus","spec":prometheus,"inputs":["PROMETHEUS_URL","PROMETHEUS_QUERY"],"requirements":"A query returning exactly one scalar or series"},
        {"id":"postgresql","title":"PostgreSQL","spec":postgres,"inputs":["PGHOST","PGPORT","PGDATABASE","PGUSER","PGPASSFILE","PGCONNECT_TIMEOUT","PGSQL_QUERY"],"requirements":"External psql and a read-only database role"},
        {"id":"docker","title":"Docker container","spec":docker,"inputs":["DOCKER_SOCKET","DOCKER_CONTAINER"],"requirements":"Trusted access to a Docker Unix socket; no Docker CLI required"}
    ]);
    for example in examples.as_array_mut().unwrap() {
        example["spec"]["required_env"] = example["inputs"].clone();
    }
    examples
}
pub fn export(conn: &Connection, cfg: &Config) -> anyhow::Result<Value> {
    let mut items = vec![];
    for item in db::items(conn)? {
        let mut spec = serde_json::to_value(&item.spec)?;
        for field in [
            "target",
            "actions",
            "prometheus_query",
            "dns_server",
            "dns_expected",
        ] {
            spec.as_object_mut().unwrap().remove(field);
        }
        // Header configuration is an input reference, never a credential value.
        items.push(json!({"id":item.id,"spec":spec,"required_env":inputs(&item.spec.target),"target_omitted":true}));
    }
    Ok(
        json!({"format":"sacho-monitor-metadata","version":1,"site":db::site(conn,cfg)?,"items":items,"notification_policy":crate::monitoring::policy(conn)?,"omitted":["executable sources and target URLs","auth and SMTP","vault and environment values","worker credentials","subscriber addresses","webhook URLs","notification templates"]}),
    )
}

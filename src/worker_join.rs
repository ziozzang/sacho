//! Persist local credential before consuming an invitation; recover with that
//! credential when enrollment succeeded but its response/config promotion failed.
use crate::{
    enrollment::{JoinRequest, Registration},
    remote::WorkerConfig,
};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    path::Path,
};

async fn registration(response: reqwest::Response) -> Result<Registration> {
    ensure!(
        response.status().is_success(),
        "worker enrollment rejected; check invitation and node ID"
    );
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= 8192,
            "registration response too large"
        );
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).context("invalid registration response")
}
pub async fn join(path: &Path) -> Result<()> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = path.parent().context("configuration directory required")?;
    let lock_path = parent.join(format!(
        ".{}.join.lock",
        path.file_name()
            .context("configuration filename required")?
            .to_string_lossy()
    ));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path)?;
    ensure!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "another join is using this configuration"
    );
    let original = std::fs::read(&path)?;
    let mut config: WorkerConfig = toml::from_str(std::str::from_utf8(&original)?)?;
    let client = crate::remote::worker_client(&config)?;
    ensure!(
        crate::secrets::valid_name(&config.token_env)
            && !config.probe_env.contains(&config.token_env),
        "invalid credential reference or probe exposure"
    );
    let validation = crate::config::Config {
        probe_env: config.probe_env.clone(),
        ..Default::default()
    };
    validation.validate()?;
    let secrets = config
        .secrets
        .get_or_insert_with(|| crate::config::Secrets {
            vault: parent
                .join(format!(".{}-credentials.enc", config.node_id))
                .to_string_lossy()
                .into_owned(),
            key_file: parent
                .join(format!(".{}-unlock.key", config.node_id))
                .to_string_lossy()
                .into_owned(),
        });
    for value in [&mut secrets.vault, &mut secrets.key_file] {
        let value_path = Path::new(value);
        if !value_path.is_absolute() {
            *value = std::env::current_dir()?
                .join(value_path)
                .to_string_lossy()
                .into_owned();
        }
    }
    let vault = Path::new(&secrets.vault);
    let key = Path::new(&secrets.key_file);
    if !vault.exists() && !key.exists() {
        crate::secrets::init(vault, key)?;
    }
    let values = crate::secrets::load(vault, key)?;
    let mut token = zeroize::Zeroizing::new(if let Some(value) = values.get(&config.token_env) {
        value.clone()
    } else {
        let value = crate::enrollment::secret();
        crate::secrets::update(vault, key, &config.token_env, Some(value.clone()))?;
        value
    });
    ensure!(
        (32..=512).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic()),
        "invalid existing worker credential"
    );
    ensure!(
        std::env::var(&config.token_env)
            .ok()
            .is_none_or(|value| value == *token),
        "environment overrides the saved worker credential; remove the override before joining"
    );
    let base = config.control_url.trim_end_matches('/');
    let status = client
        .post(format!(
            "{base}/api/workers/{}/registration",
            config.node_id
        ))
        .bearer_auth(&*token)
        .send()
        .await?;
    let response = if status.status().is_success() {
        registration(status).await?
    } else {
        ensure!(
            matches!(status.status().as_u16(), 401 | 410),
            "controller does not support recoverable enrollment or the node uses static registration"
        );
        if status.status().as_u16() == 410 {
            *token = crate::enrollment::secret();
            crate::secrets::update(vault, key, &config.token_env, Some(token.to_string()))?;
            ensure!(
                std::env::var(&config.token_env)
                    .ok()
                    .is_none_or(|value| value == *token),
                "environment overrides rotated credential; remove the override and retry"
            );
        }
        let mut bootstrap = zeroize::Zeroizing::new(String::new());
        std::io::stdin().take(513).read_to_string(&mut bootstrap)?;
        let bootstrap = bootstrap.trim();
        ensure!(
            bootstrap.len() == 64 && bootstrap.bytes().all(|b| b.is_ascii_hexdigit()),
            "read a one-time bootstrap key from stdin"
        );
        let request = JoinRequest {
            protocol: 1,
            node_id: config.node_id.clone(),
            label: if config.label.is_empty() {
                config.node_id.clone()
            } else {
                config.label.clone()
            },
            version: crate::VERSION.into(),
            platform: crate::TARGET.into(),
            token_hash: crate::enrollment::hash(&token),
            allow_scripts: config.allow_scripts,
        };
        registration(
            client
                .post(format!("{base}/api/workers/enroll"))
                .bearer_auth(bootstrap)
                .json(&request)
                .send()
                .await?,
        )
        .await?
    };
    ensure!(
        response.protocol == 1 && response.node_id == config.node_id,
        "controller returned an incompatible registration"
    );
    uuid::Uuid::parse_str(&response.controller_id)?;
    if let Some(expected) = &config.controller_id {
        ensure!(
            expected == &response.controller_id,
            "controller identity changed"
        );
    }
    config.controller_id = Some(response.controller_id.clone());
    ensure!(
        std::fs::read(&path)? == original,
        "worker configuration changed during join; retry to recover registration"
    );
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    output.write_all(toml::to_string_pretty(&config)?.as_bytes())?;
    output.as_file().sync_all()?;
    output.persist(&path).map_err(|e| e.error)?;
    std::fs::File::open(parent)?.sync_all()?;
    println!(
        "{}",
        serde_json::json!({"registered":true,"node_id":config.node_id,"controller_id":response.controller_id,"allow_scripts":response.allow_scripts})
    );
    Ok(())
}

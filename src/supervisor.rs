use crate::{
    config::Config,
    update::{ActivatedUpdate, StagedUpdate, TrustKey, UpToDate, UpdateManager},
};
use anyhow::{Context, Result, ensure};
use std::{fs::File, path::Path, time::Duration};
use tokio::process::{Child, Command};

pub async fn signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=term.recv()=>{} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
pub fn instance_lock(database: &str) -> Result<File> {
    let path = format!("{database}.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        ensure!(
            result == 0,
            "another Sacho server or update is using this database"
        );
    }
    Ok(file)
}
pub async fn stage(cfg: &Config, exe: &Path) -> Result<StagedUpdate> {
    stage_for(cfg, exe, crate::VERSION).await
}
async fn stage_for(cfg: &Config, exe: &Path, current: &str) -> Result<StagedUpdate> {
    let key = cfg
        .updates
        .public_key
        .as_deref()
        .unwrap_or(include_str!("../release-public-key.txt"));
    let trust = TrustKey::from_base64(key)?;
    let manager = if cfg.updates.manifest_url.is_some() {
        UpdateManager::new_with_ca(
            trust,
            semver::Version::parse(current)?,
            crate::TARGET,
            cfg.updates
                .ca_file
                .as_ref()
                .map(|path| -> Result<_> {
                    Ok(reqwest::Certificate::from_pem(&std::fs::read(path)?)?)
                })
                .transpose()?,
        )?
    } else {
        UpdateManager::new_github(trust, semver::Version::parse(current)?, crate::TARGET)?
    };
    let dir = exe.parent().context("binary has no parent")?;
    let staged = if let Some(url) = &cfg.updates.manifest_url {
        manager.stage(url, dir).await?
    } else {
        manager.stage_github(dir).await?
    };
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(staged.path())
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    ensure!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).trim()
                == format!("sacho {}", staged.version()),
        "signed candidate failed version preflight"
    );
    Ok(staged)
}
async fn stop(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    if tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}
async fn start(exe: &Path, config: &Path) -> Result<(Child, String)> {
    let temp = tempfile::tempdir()?;
    let ready = temp.path().join("ready");
    let mut child = Command::new(exe)
        .args(["serve", "--worker", "--config"])
        .arg(config)
        .arg("--ready-file")
        .arg(&ready)
        .kill_on_drop(true)
        .spawn()?;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("worker exited during startup: {status}");
            }
            if ready.exists() {
                let version = std::fs::read_to_string(&ready)?;
                semver::Version::parse(version.trim())?;
                return Ok::<_, anyhow::Error>(version.trim().to_owned());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    match result {
        Ok(Ok(version)) => Ok((child, version)),
        Ok(Err(error)) => {
            stop(&mut child).await;
            Err(error)
        }
        Err(_) => {
            stop(&mut child).await;
            anyhow::bail!("worker readiness timed out");
        }
    }
}
pub async fn run(config: &Path, cfg: Config) -> Result<()> {
    let _lock = instance_lock(&cfg.database)?;
    let exe = std::env::current_exe()?;
    let config = std::fs::canonicalize(config)?;
    let (mut child, mut version) = start(&exe, &config).await?;
    let mut timer = tokio::time::interval(Duration::from_secs(cfg.updates.interval_seconds));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut retained: Option<ActivatedUpdate> = None;
    let mut healthy_since = tokio::time::Instant::now();
    let mut health = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _=signal()=>{stop(&mut child).await;return Ok(());}
            status=child.wait()=>{
                let status=status?;
                if let Some(activated)=retained.take() {
                    tracing::error!(%status,"updated worker exited during probation; rolling back");
                    UpdateManager::rollback(activated)?;
                    (child,version)=start(&exe,&config).await?;
                } else {ensure!(status.success(),"worker exited: {status}");return Ok(());}
            }
            _=health.tick()=>{
                if retained.is_some() && healthy_since.elapsed()>=Duration::from_secs(30) {
                    // Ready file proves a database open and successful bind; probation
                    // additionally requires the new worker to remain alive for 30s.
                    if let Some(activated)=retained.take() {activated.discard_rollback()?;tracing::info!(%version,"automatic update healthy");}
                }
            }
            _=timer.tick(), if cfg.updates.enabled && retained.is_none()=>{
                let result=tokio::select! {
                    _=signal()=>{stop(&mut child).await;return Ok(());},
                    status=child.wait()=>{anyhow::bail!("worker exited while checking updates: {}",status?);},
                    result=stage_for(&cfg,&exe,&version)=>result,
                };
                match result {
                    Ok(staged)=>{
                        tracing::info!(version=%staged.version(),"verified automatic update; restarting worker");
                        stop(&mut child).await;
                        let activated=match UpdateManager::activate(staged,&exe) {
                            Ok(a)=>a,
                            Err(error)=>{(child,version)=start(&exe,&config).await?;tracing::error!(%error,"update activation failed");continue;}
                        };
                        match start(&exe,&config).await {
                            Ok((new_child,new_version))=>{child=new_child;version=new_version;healthy_since=tokio::time::Instant::now();retained=Some(activated);}
                            Err(error)=>{tracing::error!(%error,"updated worker failed readiness; rolling back");UpdateManager::rollback(activated)?;(child,version)=start(&exe,&config).await?;}
                        }
                    }
                    Err(error) if error.downcast_ref::<UpToDate>().is_some()=>tracing::debug!("already up to date"),
                    Err(error)=>tracing::warn!(%error,reason=%error.root_cause(),"automatic update rejected or unavailable"),
                }
            }
        }
    }
}

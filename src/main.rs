use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sacho", version, about = "Service health, recorded with care.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    /// Write an example configuration
    Init {
        #[arg(long, default_value = "sacho.toml")]
        config: PathBuf,
    },
    /// Run the status page and monitoring engine
    Serve {
        #[arg(long, default_value = "sacho.toml")]
        config: PathBuf,
        #[arg(long, hide = true)]
        worker: bool,
        #[arg(long, hide = true)]
        ready_file: Option<PathBuf>,
    },
    /// Check the latest published release
    Update {
        #[arg(long, default_value = "sacho.toml")]
        config: PathBuf,
        #[arg(long)]
        check: bool,
    },
    /// Validate configuration and database
    Doctor {
        #[arg(long, default_value = "sacho.toml")]
        config: PathBuf,
    },
    /// Sign a release manifest; key comes from SACHO_RELEASE_SIGNING_KEY
    SignRelease {
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    #[command(hide = true)]
    ProbeLua,
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sacho=info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Serve {
        config: "sacho.toml".into(),
        worker: false,
        ready_file: None,
    }) {
        Commands::Init { config } => {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&config)?;
            file.write_all(include_bytes!("../examples/sacho.toml"))?;
            println!(
                "Created {}. Run: sacho serve --config {}",
                config.display(),
                config.display()
            );
        }
        Commands::Serve {
            config,
            worker,
            ready_file,
        } => {
            let cfg = sacho::config::Config::load(&config)?;
            if worker {
                sacho::server::serve(cfg, ready_file).await?;
            } else {
                sacho::supervisor::run(&config, cfg).await?;
            }
        }
        Commands::Doctor { config } => {
            let cfg = sacho::config::Config::load(&config)?;
            let db = sacho::db::open(&cfg.database)?;
            let conn = db.lock().unwrap();
            for item in sacho::db::items(&conn)? {
                item.spec.validate()?;
            }
            let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            anyhow::ensure!(integrity == "ok", "database integrity check failed");
            println!(
                "Configuration and database OK; sacho {} ({})",
                sacho::VERSION,
                sacho::TARGET
            );
        }
        Commands::Update { config, check } => {
            if check {
                let r = sacho::github_release::latest().await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &r.check(semver::Version::parse(sacho::VERSION)?, sacho::TARGET)?
                    )?
                );
            } else {
                let cfg = sacho::config::Config::load(&config)?;
                let _lock = sacho::supervisor::instance_lock(&cfg.database)?;
                let exe = std::env::current_exe()?;
                let staged = sacho::supervisor::stage(&cfg, &exe).await?;
                let activated = sacho::update::UpdateManager::activate(staged, &exe)?;
                println!(
                    "Installed {}. Rollback retained at {}. Restart your running service.",
                    activated.version(),
                    activated.rollback_path().display()
                );
            }
        }
        Commands::SignRelease { payload, output } => {
            let key = sacho::update::signing_key_from_base64(
                &std::env::var("SACHO_RELEASE_SIGNING_KEY")
                    .context("SACHO_RELEASE_SIGNING_KEY missing")?,
            )?;
            let signed = sacho::update::sign_release_manifest(&std::fs::read(payload)?, &key)?;
            std::fs::write(output, signed)?;
        }
        Commands::ProbeLua => {
            use std::io::Read;
            let mut source = String::new();
            std::io::stdin().take(65537).read_to_string(&mut source)?;
            anyhow::ensure!(source.len() <= 65536, "Lua source exceeds 64 KiB");
            tokio::task::spawn_blocking(move || sacho::engine::lua_probe(&source)).await??;
        }
    }
    Ok(())
}

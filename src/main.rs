use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sacho",
    version,
    about = "Service health, recorded with care.",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Clone, Copy, ValueEnum)]
enum GuideLanguage {
    En,
    Ko,
}
fn setup_guide(language: GuideLanguage) -> &'static str {
    match language {
        GuideLanguage::En => include_str!("../docs/SETUP-HOWTO.md"),
        GuideLanguage::Ko => include_str!("../docs/SETUP-HOWTO.ko.md"),
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum ExampleName {
    Prometheus,
    Postgresql,
    ExternalCommand,
}
#[derive(Subcommand)]
enum SecretAction {
    /// Create a new encrypted vault and random unlock key
    Init,
    /// Store a value read from stdin; never put secrets in argv
    Set { name: String },
    /// List names only, never values
    List,
    /// Remove a named value
    Remove { name: String },
}
#[derive(Subcommand)]
enum Commands {
    /// Print an embedded Lua data-source example
    Example {
        #[arg(value_enum)]
        name: ExampleName,
    },
    /// Manage encrypted generic inputs (not provider credentials)
    Secrets {
        #[command(subcommand)]
        action: SecretAction,
        #[arg(long, global = true)]
        vault: Option<PathBuf>,
        #[arg(long, global = true)]
        key_file: Option<PathBuf>,
    },
    /// Print the embedded setup guide for operators and LLM agents
    SetupHowto {
        #[arg(long, value_enum, default_value = "en")]
        language: GuideLanguage,
    },
    /// Show command help, or the embedded guide with `help setup`
    Help {
        topic: Option<String>,
        #[arg(long, value_enum, default_value = "en")]
        language: GuideLanguage,
    },
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
        Commands::Secrets {
            action,
            vault,
            key_file,
        } => {
            let vault = vault.context("--vault is required")?;
            let key_file = key_file.context("--key-file is required")?;
            match action {
                SecretAction::Init => sacho::secrets::init(&vault, &key_file)?,
                SecretAction::Set { name } => {
                    use std::io::Read;
                    let mut value = zeroize::Zeroizing::new(String::new());
                    std::io::stdin().take(65537).read_to_string(&mut value)?;
                    anyhow::ensure!(value.len() <= 65536, "secret input exceeds 64 KiB");
                    if value.ends_with('\n') {
                        value.pop();
                        if value.ends_with('\r') {
                            value.pop();
                        }
                    }
                    sacho::secrets::update(&vault, &key_file, &name, Some(value.to_string()))?;
                }
                SecretAction::List => {
                    for name in sacho::secrets::load(&vault, &key_file)?.keys() {
                        println!("{name}");
                    }
                }
                SecretAction::Remove { name } => {
                    sacho::secrets::update(&vault, &key_file, &name, None)?
                }
            }
        }
        Commands::Example { name } => {
            use std::io::Write;
            let source = match name {
                ExampleName::Prometheus => include_str!("../examples/lua/prometheus.lua"),
                ExampleName::Postgresql => include_str!("../examples/lua/postgresql.lua"),
                ExampleName::ExternalCommand => {
                    include_str!("../examples/lua/external-command.lua")
                }
            };
            std::io::stdout().write_all(source.as_bytes())?;
        }
        Commands::SetupHowto { language } => {
            use std::io::Write;
            std::io::stdout().write_all(setup_guide(language).as_bytes())?;
        }
        Commands::Help { topic, language } => {
            if matches!(topic.as_deref(), Some("setup" | "setup-howto")) {
                use std::io::Write;
                std::io::stdout().write_all(setup_guide(language).as_bytes())?;
            } else if topic.as_deref() == Some("licenses") {
                use std::io::Write;
                std::io::stdout().write_all(
                    concat!(
                        include_str!("../THIRD_PARTY.md"),
                        "\n",
                        include_str!("../docs/licenses/minijinja.txt"),
                        "\n",
                        include_str!("../docs/licenses/rustcrypto-mit.txt"),
                        "\n",
                        include_str!("../docs/licenses/zeroize-mit.txt")
                    )
                    .as_bytes(),
                )?;
            } else {
                let mut command = Cli::command();
                if let Some(topic) = topic {
                    command
                        .find_subcommand_mut(&topic)
                        .with_context(|| {
                            format!("Unknown help topic: {topic}; try `sacho help setup`")
                        })?
                        .print_long_help()?;
                } else {
                    command.print_long_help()?;
                }
                println!();
            }
        }
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

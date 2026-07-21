mod config;
mod daemon;

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use tailr_server::app;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "tailr", version, about = "Log tail and search server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Args)]
struct ServeArgs {
    /// Custom config file path
    #[arg(long)]
    config: Option<PathBuf>,

    /// Log directories or files to serve (can specify multiple)
    #[arg(short, long, num_args = 1..)]
    log: Vec<PathBuf>,

    /// Bind address
    #[arg(short, long)]
    bind: Option<String>,

    /// Run as daemon in background
    #[arg(long, short)]
    daemon: bool,

    /// Custom PID file path
    #[arg(long)]
    pid_file: Option<PathBuf>,

    /// Custom log file path for daemon mode
    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize config file (prompt to confirm if file exists)
    Init {
        /// Custom config file path
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Print config file contents
    Config {
        /// Custom config file path
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Stop running daemon
    Stop {
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },

    /// Restart running daemon (stops then re-execs with the same args)
    Restart {
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },

    /// Show daemon status
    Status {
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },

    /// Generate systemd service file
    Systemd {
        /// Log directories or files to serve
        #[arg(short, long, num_args = 1..)]
        log: Vec<PathBuf>,

        /// User to run the service as
        #[arg(long, default_value_t = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string()))]
        user: String,

        /// Group to run the service as
        #[arg(long)]
        group: Option<String>,
    },

    /// Generate launchd plist file (macOS)
    Launchd {
        /// Log directories or files to serve
        #[arg(short, long, num_args = 1..)]
        log: Vec<PathBuf>,
    },

    /// Check for updates and upgrade tailr to the latest version
    Upgrade {
        /// Only check for updates without installing
        #[arg(long, short = 'c')]
        check: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { config }) => {
            let config_path = config::resolve_config_path(config.as_ref());
            if config_path.exists() {
                println!("Config file already exists: {}", config_path.display());
                println!("Overwrite? [y/N] ");
                let mut answer = String::new();
                if std::io::stdin().read_line(&mut answer).is_err() {
                    eprintln!("Failed to read input");
                    std::process::exit(1);
                }
                if answer.trim().to_lowercase() != "y" {
                    println!("Aborted.");
                    return;
                }
            }
            match config::write_default_config(&config_path) {
                Ok(()) => println!("Config file created: {}", config_path.display()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Config { config }) => {
            let config_path = config::resolve_config_path(config.as_ref());
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => print!("{}", contents),
                Err(e) => {
                    eprintln!("Config file not found: {} ({})", config_path.display(), e);
                    eprintln!("Run `tailr init` to create one.");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Stop { pid_file }) => {
            match daemon::stop_daemon(pid_file.as_ref(), 5) {
                Ok(()) => println!("tailr stopped"),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Restart { pid_file }) => {
            match daemon::restart_daemon(pid_file.as_ref()) {
                Ok(()) => println!("tailr restarted"),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Status { pid_file }) => {
            println!("{}", daemon::daemon_status(pid_file.as_ref()));
        }
        Some(Commands::Systemd { log, user, group }) => {
            let binary = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "tailr".to_string());
            let log_dirs: Vec<String> = log.iter().map(|p| p.display().to_string()).collect();
            let group = group.unwrap_or_else(|| user.clone());
            println!("{}", daemon::generate_systemd_service(&binary, &log_dirs, &user, &group));
        }
        Some(Commands::Launchd { log }) => {
            let binary = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "tailr".to_string());
            let log_dirs: Vec<String> = log.iter().map(|p| p.display().to_string()).collect();
            println!("{}", daemon::generate_launchd_plist(&binary, &log_dirs));
        }
        Some(Commands::Upgrade { check }) => {
            if let Err(e) = run_upgrade(check) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            run_server(cli.serve);
        }
    }
}

fn run_server(args: ServeArgs) {
    let config_path = config::resolve_config_path(args.config.as_ref());

    if let Err(e) = config::ensure_config_file(&config_path) {
        eprintln!("Warning: {}", e);
    }

    let cfg = config::load_config(
        &config_path,
        optional_vec(&args.log),
        args.bind.as_deref(),
        args.daemon,
        args.pid_file.as_ref(),
        args.log_file.as_ref(),
    )
    .expect("Failed to load configuration");

    // Handle daemon mode BEFORE tokio runtime starts
    // daemonize() forks the process, which is incompatible with an async runtime
    if args.daemon {
        let daemon_cfg = daemon::DaemonConfig {
            pid_file: cfg.daemon.pid_file.clone().unwrap_or_else(daemon::pid_file),
            log_file: cfg.daemon.log_file.clone().unwrap_or_else(daemon::log_file),
            working_dir: std::env::current_dir().unwrap_or_else(|_| daemon::data_dir()),
        };
        daemon::daemonize_process(&daemon_cfg);
    }

    // Now start tokio runtime (in the daemon child process if daemonized)
    // Persist the server's invocation so `tailr restart` can re-launch it with
    // the same args. Done after daemonize so the daemon child's args are recorded.
    daemon::save_restart_cmd();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env()
                    .add_directive("tailr=info".parse().unwrap()),
            )
            .init();

        run_serve(cfg, config_path).await;
    });
}

async fn run_serve(cfg: config::Config, config_path: PathBuf) {
    let log_paths = config::resolve_log_paths(&cfg);

    for path in &log_paths {
        if !path.exists() {
            tracing::warn!(path = %path.display(), "path does not exist");
        }
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await.unwrap();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "tailr listening on {} (paths: {:?})",
        listener.local_addr().unwrap(),
        log_paths
    );

    let level_config = cfg.log_levels.clone()
        .unwrap_or_else(|| config::default_log_levels("general"));

    let log_timezone = tailr_protocol::LogTimezone::parse(&cfg.log_timezone)
        .unwrap_or_else(|e| {
            tracing::warn!(
                "invalid log_timezone '{}': {}, using local",
                cfg.log_timezone,
                e
            );
            tailr_protocol::LogTimezone::default()
        });

    let server = axum::serve(
        listener,
        app(
            log_paths,
            config_path,
            level_config,
            log_timezone,
            cfg.token.clone(),
            cfg.limits.clone(),
        ),
    )
    .with_graceful_shutdown(daemon::shutdown_signal());

    if let Err(e) = server.await {
        tracing::error!("server error: {}", e);
    }

    daemon::cleanup_pid_file(cfg.daemon.pid_file.as_ref());
}

/// CLI entry for `tailr upgrade` / `tailr upgrade --check`.
///
/// Delegates all `self_update` logic to [`tailr_server::upgrade::UpgradeEngine`]
/// (the single source of truth shared with the Web UI). This function only handles
/// CLI ergonomics: platform error message, printing, and the post-upgrade hint.
fn run_upgrade(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let engine = tailr_server::upgrade::UpgradeEngine::new();

    if !engine.supported() {
        return Err(
            "Self-upgrade is only supported on Linux.\n\
             Please download the latest release manually from:\n\
             https://github.com/flolibio/tailr/releases"
                .into(),
        );
    }

    let info = engine.check_update().map_err(|e| -> Box<dyn std::error::Error> {
        e.into()
    })?;

    if !info.has_update {
        println!("Already up to date (v{})", info.current_version);
        return Ok(());
    }

    if check {
        println!(
            "New version available: v{} (current: v{})\n\
             Run `tailr upgrade` to install the update.",
            info.latest_version, info.current_version
        );
        return Ok(());
    }

    let version = engine.perform_upgrade().map_err(|e| -> Box<dyn std::error::Error> {
        e.into()
    })?;
    println!(
        "Updated to v{}! Run `tailr restart` to apply the new version.",
        version
    );

    Ok(())
}

/// Returns `Some` for non-empty Vec, `None` otherwise (so figment uses lower priority).
fn optional_vec(v: &[PathBuf]) -> Option<Vec<PathBuf>> {
    if v.is_empty() {
        None
    } else {
        Some(v.to_vec())
    }
}

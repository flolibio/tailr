mod daemon;

use clap::{Args, Parser, Subcommand};
use self_update::cargo_crate_version;
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
    /// Log directories or files to serve (can specify multiple)
    #[arg(short, long, num_args = 1..)]
    log: Vec<PathBuf>,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0:7700")]
    bind: String,

    /// Run as daemon in background
    #[arg(long, short)]
    daemon: bool,

    /// Stop running daemon
    #[arg(long)]
    stop: bool,

    /// Show daemon status
    #[arg(long)]
    status: bool,

    /// Print systemd service file and exit
    #[arg(long)]
    systemd: bool,

    /// Print launchd plist file and exit (macOS)
    #[arg(long)]
    launchd: bool,

    /// Custom PID file path
    #[arg(long)]
    pid_file: Option<PathBuf>,

    /// Custom log file path for daemon mode
    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check for updates and upgrade tailr to the latest version
    Upgrade {
        /// Only check for updates without installing
        #[arg(long, short = 'c')]
        check: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    // Handle non-server commands first (no tokio needed)
    if cli.serve.stop {
        match daemon::stop_daemon(cli.serve.pid_file.as_ref(), 5) {
            Ok(()) => println!("tailr stopped"),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.serve.status {
        println!("{}", daemon::daemon_status(cli.serve.pid_file.as_ref()));
        return;
    }

    if cli.serve.systemd {
        let binary = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "tailr".to_string());
        let log_dirs: Vec<String> = cli.serve.log.iter().map(|p| p.display().to_string()).collect();
        let user = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string());
        println!("{}", daemon::generate_systemd_service(&binary, &log_dirs, &user, &user));
        return;
    }

    if cli.serve.launchd {
        let binary = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "tailr".to_string());
        let log_dirs: Vec<String> = cli.serve.log.iter().map(|p| p.display().to_string()).collect();
        println!("{}", daemon::generate_launchd_plist(&binary, &log_dirs));
        return;
    }

    // Handle daemon mode BEFORE tokio runtime starts
    // daemonize() forks the process, which is incompatible with an async runtime
    if cli.serve.daemon {
        let config = daemon::DaemonConfig {
            pid_file: cli.serve.pid_file.clone().unwrap_or_else(daemon::pid_file),
            log_file: cli.serve.log_file.clone().unwrap_or_else(daemon::log_file),
            working_dir: std::env::current_dir().unwrap_or_else(|_| daemon::data_dir()),
        };
        daemon::daemonize_process(&config);
    }

    // Now start tokio runtime (in the daemon child process if daemonized)
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env()
                    .add_directive("tailr=info".parse().unwrap()),
            )
            .init();

        match cli.command {
            Some(Commands::Upgrade { check }) => {
                if let Err(e) = run_upgrade(check) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            None => {
                run_serve(cli.serve).await;
            }
        }
    });
}

async fn run_serve(args: ServeArgs) {
    let log_paths: Vec<PathBuf> = if args.log.is_empty() {
        std::env::var("TAILR_LOG_DIR")
            .map(|val| val.split(',').map(|s| PathBuf::from(s.trim())).collect())
            .unwrap_or_else(|_| {
                let default = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("logs");
                vec![default]
            })
    } else {
        args.log
    };

    for path in &log_paths {
        if !path.exists() {
            tracing::warn!(path = %path.display(), "path does not exist");
        }
    }

    let listener = tokio::net::TcpListener::bind(&args.bind).await.unwrap();
    tracing::info!(
        "tailr listening on {} (paths: {:?})",
        listener.local_addr().unwrap(),
        log_paths
    );

    let server = axum::serve(listener, app(log_paths))
        .with_graceful_shutdown(daemon::shutdown_signal());

    if let Err(e) = server.await {
        tracing::error!("server error: {}", e);
    }

    daemon::cleanup_pid_file(args.pid_file.as_ref());
}

fn run_upgrade(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    if std::env::consts::OS != "linux" {
        return Err(
            "Self-upgrade is only supported on Linux.\n\
             Please download the latest release manually from:\n\
             https://github.com/wunamesst/tailr/releases"
                .into(),
        );
    }

    let target = match std::env::consts::ARCH {
        "x86_64" => "x86_64-linux-musl",
        "aarch64" => "aarch64-linux-musl",
        arch => {
            return Err(format!(
                "Unsupported architecture: {}.\nSupported architectures: x86_64, aarch64",
                arch
            )
            .into())
        }
    };

    let updater = self_update::backends::github::Update::configure()
        .repo_owner("wunamesst")
        .repo_name("tailr")
        .bin_name("tailr")
        .target(target)
        .current_version(cargo_crate_version!())
        .no_confirm(true)
        .show_download_progress(true)
        .build()?;

    if check {
        let release = updater.get_latest_release()?;
        let current = cargo_crate_version!();
        if self_update::version::bump_is_greater(current, &release.version)? {
            println!(
                "New version available: v{} (current: v{})\n\
                 Run `tailr upgrade` to install the update.",
                release.version, current
            );
        } else {
            println!("Already up to date (v{})", current);
        }
    } else {
        let status = updater.update()?;
        match status {
            self_update::Status::UpToDate(version) => {
                println!("Already up to date (v{})", version);
            }
            self_update::Status::Updated(version) => {
                println!(
                    "Updated to v{}! Please restart the service to use the new version.",
                    version
                );
            }
        }
    }

    Ok(())
}

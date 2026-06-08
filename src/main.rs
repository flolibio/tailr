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
}

#[derive(Subcommand)]
enum Commands {
    /// Check for updates and upgrade tailr to the latest version
    Upgrade {
        /// Only check for updates without installing
        #[arg(long)]
        check_only: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("tailr=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Upgrade { check_only }) => {
            if let Err(e) = run_upgrade(check_only) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            run_serve(cli.serve).await;
        }
    }
}

async fn run_serve(args: ServeArgs) {
    let log_paths: Vec<PathBuf> = if args.log.is_empty() {
        // Fallback to env var or default
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

    // Validate paths exist
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
    axum::serve(listener, app(log_paths)).await.unwrap();
}

fn run_upgrade(check_only: bool) -> Result<(), Box<dyn std::error::Error>> {
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

    if check_only {
        let release = updater.get_latest_release()?;
        let current = cargo_crate_version!();
        if release.version == current {
            println!("Already up to date (v{})", current);
        } else {
            println!(
                "New version available: v{} (current: v{})\n\
                 Run `tailr upgrade` to install the update.",
                release.version, current
            );
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

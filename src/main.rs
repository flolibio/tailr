use clap::Parser;
use logtailer_server::app;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "logtailer", version, about = "Log tail and search server")]
struct Cli {
    /// Log directories or files to serve (can specify multiple)
    #[arg(short, long, num_args = 1..)]
    log: Vec<PathBuf>,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0:3000")]
    bind: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("logtailer=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let log_paths: Vec<PathBuf> = if cli.log.is_empty() {
        // Fallback to env var or default
        std::env::var("LOGTAILER_LOG_DIR")
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
        cli.log
    };

    // Validate paths exist
    for path in &log_paths {
        if !path.exists() {
            tracing::warn!(path = %path.display(), "path does not exist");
        }
    }

    let listener = tokio::net::TcpListener::bind(&cli.bind).await.unwrap();
    tracing::info!(
        "logtailer listening on {} (paths: {:?})",
        listener.local_addr().unwrap(),
        log_paths
    );
    axum::serve(listener, app(log_paths)).await.unwrap();
}

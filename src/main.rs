use logtailer_server::app;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("logtailer=debug".parse().unwrap()),
        )
        .init();

    let log_dirs: Vec<PathBuf> = std::env::var("LOGTAILER_LOG_DIR")
        .map(|val| val.split(',').map(|s| PathBuf::from(s.trim())).collect())
        .unwrap_or_else(|_| {
            // Default to ./logs relative to the binary's location
            let default = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."))
                .join("logs");
            vec![default]
        });

    let bind = std::env::var("LOGTAILER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    tracing::info!(
        "logtailer listening on {} (log_dirs: {:?})",
        listener.local_addr().unwrap(),
        log_dirs
    );
    axum::serve(listener, app(log_dirs)).await.unwrap();
}

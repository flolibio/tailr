pub mod api;
pub mod static_files;
pub mod upgrade;
pub mod ws;

use arc_swap::ArcSwap;
use axum::extract::{Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use dashmap::DashMap;
use tailr_protocol::{LogLevelConfig, LogTimezone};
use tailr_search_engine::{LevelDetector, SearchEngine};
use tailr_tail_engine::{FileWatcher, LineIndex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

pub struct AppState {
    pub watcher: Arc<Mutex<FileWatcher>>,
    pub search_engine: SearchEngine,
    pub line_indices: DashMap<PathBuf, LineIndex>,
    pub file_subscribers: Mutex<HashMap<String, ws::FileSubscribers>>,
    /// Global WS client registry (client_id → sender), independent of file
    /// subscriptions. Used to broadcast server-wide notifications like
    /// `UpdateAvailable` to every connected client.
    pub ws_clients: Mutex<HashMap<String, tokio::sync::mpsc::Sender<tailr_protocol::WSMessage>>>,
    pub log_dirs: Vec<PathBuf>,
    pub log_files: Vec<PathBuf>,
    pub start_time: Instant,
    pub level_config: Arc<ArcSwap<LogLevelConfig>>,
    pub level_detector: Arc<ArcSwap<LevelDetector>>,
    pub config_path: PathBuf,
    pub token: String,
    pub allowed_dirs: Vec<PathBuf>,
    pub log_timezone: Arc<LogTimezone>,
    pub upgrade_service: Arc<upgrade::UpgradeService>,
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if state.token.is_empty() {
        return next.run(request).await;
    }

    let auth = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth == format!("Bearer {}", state.token) {
        return next.run(request).await;
    }

    // WebSocket: browsers can't set custom headers, allow token via query param
    if request.uri().path() == "/ws" {
        if let Some(query) = request.uri().query() {
            for pair in query.split('&') {
                if let Some(t) = pair.strip_prefix("token=") {
                    if t == state.token {
                        return next.run(request).await;
                    }
                }
            }
        }
    }

    StatusCode::UNAUTHORIZED.into_response()
}

pub fn app(
    log_paths: Vec<PathBuf>,
    config_path: PathBuf,
    level_config: LogLevelConfig,
    log_timezone: LogTimezone,
    token: String,
) -> Router {
    let level_detector = LevelDetector::from_config(&level_config);
    let level_detector_arc = Arc::new(ArcSwap::from_pointee(level_detector));
    let log_timezone_arc = Arc::new(log_timezone);

    let watcher = FileWatcher::new(
        Duration::from_millis(100),
        level_detector_arc.clone(),
        log_timezone_arc.clone(),
    )
    .expect("failed to create FileWatcher");

    let (log_dirs, log_files): (Vec<_>, Vec<_>) = log_paths
        .into_iter()
        .partition(|p| p.is_dir());

    let allowed_dirs: Vec<PathBuf> = {
        let mut dirs = log_dirs.clone();
        for file in &log_files {
            if let Some(parent) = file.parent() {
                let canonical = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                if !dirs.contains(&canonical) {
                    dirs.push(canonical);
                }
            }
        }
        dirs
    };

    let state = Arc::new(AppState {
        watcher: Arc::new(Mutex::new(watcher)),
        search_engine: SearchEngine::new(),
        line_indices: DashMap::new(),
        file_subscribers: Mutex::new(HashMap::new()),
        ws_clients: Mutex::new(HashMap::new()),
        log_dirs,
        log_files,
        start_time: Instant::now(),
        level_config: Arc::new(ArcSwap::from_pointee(level_config)),
        level_detector: level_detector_arc,
        config_path,
        token,
        allowed_dirs,
        log_timezone: log_timezone_arc,
        upgrade_service: upgrade::shared_service(),
    });

    ws::spawn_watcher_loop(state.clone());
    state
        .upgrade_service
        .start_background_check(state.clone());

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            "X-Requested-With".parse().unwrap(),
        ]);

    Router::new()
        .merge(api::routes())
        .merge(ws::routes())
        .merge(static_files::routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(cors)
        .layer(axum::extract::Extension(state))
}

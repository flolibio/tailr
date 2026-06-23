pub mod api;
pub mod static_files;
pub mod ws;

use arc_swap::ArcSwap;
use axum::extract::{Request, State};
use axum::http::{header, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use dashmap::DashMap;
use tailr_protocol::LogLevelConfig;
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
    pub log_dirs: Vec<PathBuf>,
    pub log_files: Vec<PathBuf>,
    pub start_time: Instant,
    pub level_config: Arc<ArcSwap<LogLevelConfig>>,
    pub level_detector: Arc<ArcSwap<LevelDetector>>,
    pub config_path: PathBuf,
    pub token: String,
    pub allowed_dirs: Vec<PathBuf>,
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
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

pub fn app(
    log_paths: Vec<PathBuf>,
    config_path: PathBuf,
    level_config: LogLevelConfig,
    token: String,
) -> Router {
    let level_detector = LevelDetector::from_config(&level_config);
    let level_detector_arc = Arc::new(ArcSwap::from_pointee(level_detector));

    let watcher = FileWatcher::new(Duration::from_millis(100), level_detector_arc.clone())
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
        log_dirs,
        log_files,
        start_time: Instant::now(),
        level_config: Arc::new(ArcSwap::from_pointee(level_config)),
        level_detector: level_detector_arc,
        config_path,
        token,
        allowed_dirs,
    });

    ws::spawn_watcher_loop(state.clone());

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

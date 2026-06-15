pub mod api;
pub mod static_files;
pub mod ws;

use arc_swap::ArcSwap;
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
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub watcher: Arc<Mutex<FileWatcher>>,
    pub search_engine: SearchEngine,
    pub line_indices: DashMap<PathBuf, LineIndex>,
    pub file_subscribers: Mutex<HashMap<String, ws::FileSubscribers>>,
    pub log_dirs: Vec<PathBuf>,
    pub log_files: Vec<PathBuf>,
    pub start_time: Instant,
    /// 当前日志级别配置（无锁读，arc-swap 热更新）
    pub level_config: Arc<ArcSwap<LogLevelConfig>>,
    /// 当前动态级别检测器（无锁读，arc-swap 热更新）
    pub level_detector: Arc<ArcSwap<LevelDetector>>,
    /// config.toml 路径，用于写回配置
    pub config_path: PathBuf,
}

pub fn app(
    log_paths: Vec<PathBuf>,
    config_path: PathBuf,
    level_config: LogLevelConfig,
) -> Router {
    let level_detector = LevelDetector::from_config(&level_config);
    let level_detector_arc = Arc::new(ArcSwap::from_pointee(level_detector));

    let watcher = FileWatcher::new(Duration::from_millis(100), level_detector_arc.clone())
        .expect("failed to create FileWatcher");

    let (log_dirs, log_files): (Vec<_>, Vec<_>) = log_paths
        .into_iter()
        .partition(|p| p.is_dir());

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
    });

    ws::spawn_watcher_loop(state.clone());

    Router::new()
        .merge(api::routes())
        .merge(ws::routes())
        .merge(static_files::routes())
        .layer(CorsLayer::permissive())
        .layer(axum::extract::Extension(state))
}

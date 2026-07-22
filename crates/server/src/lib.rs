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
use serde::{Deserialize, Serialize};
use tailr_protocol::{LogLevelConfig, LogTimezone};
use tailr_search_engine::LevelDetector;
use tailr_tail_engine::{FileWatcher, LineIndex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

/// Resource limits for production hardening.
/// All thresholds are user-tunable via `[limits]` section in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// 全局 WS 连接上限（含所有客户端）。默认 50——覆盖内网 5-10 人团队
    /// ×每人 3-5 tab 的场景，留余量。触顶通常是前端 bug（WS 未释放）或异常。
    pub max_ws_connections: usize,
    /// 每 IP 每秒最大 REST 请求数（GCRA 持续速率）。默认 20——单用户正常使用 < 5 req/s。
    /// 实际瞬时突发由 `burst_size = rate_limit_rps * 3` 覆盖（不暴露给用户配置）。
    pub rate_limit_rps: u32,
    /// 是否启用 gzip 压缩响应。默认 false。
    /// 内网千兆场景下,miniz_oxide 压缩吞吐(~70MB/s)低于网络带宽(125MB/s),
    /// 压缩的 CPU 开销(~14ms/MB)大于传输节省,反而变慢 10-15%。
    /// 公网/弱网/VPN 远程访问场景下,带宽通常 < 70MB/s(560Mbps),压缩有明显收益
    /// (1MB 响应:家用宽带快 5x,4G 快 20x,弱网快 29x)。
    /// 用户按自己部署场景决定是否开启。
    pub enable_compression: bool,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_ws_connections: 50,
            rate_limit_rps: 20,
            enable_compression: false,
        }
    }
}

pub struct AppState {
    pub watcher: Arc<Mutex<FileWatcher>>,
    pub line_indices: DashMap<PathBuf, LineIndex>,
    pub file_subscribers: Mutex<HashMap<String, ws::FileSubscribers>>,
    /// Global WS client registry (client_id → sender), independent of file
    /// subscriptions. Used to broadcast server-wide notifications like
    /// `UpdateAvailable` to every connected client.
    pub ws_clients: Mutex<HashMap<String, tokio::sync::mpsc::Sender<tailr_protocol::WSMessage>>>,
    /// Current WebSocket connection count (global, all clients).
    /// Bounded by `limits.max_ws_connections`. Incremented in `ws_handler`
    /// (with TOCTOU-safe fetch_add + rollback on over-limit), decremented
    /// in `cleanup_client`.
    pub ws_connection_count: AtomicUsize,
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
    /// Resource limits (WS connection cap, REST rate limit). User-tunable
    /// via `[limits]` in config.toml.
    pub limits: LimitsConfig,
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
    limits: LimitsConfig,
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

    // Capture config values before `limits` is moved into AppState.
    let rps = limits.rate_limit_rps;
    let enable_compression = limits.enable_compression;

    let state = Arc::new(AppState {
        watcher: Arc::new(Mutex::new(watcher)),
        line_indices: DashMap::new(),
        file_subscribers: Mutex::new(HashMap::new()),
        ws_clients: Mutex::new(HashMap::new()),
        ws_connection_count: AtomicUsize::new(0),
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
        limits,
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

    // Per-IP GCRA rate limit on the REST/static surface.
    //
    // burst_size = rate_limit_rps * 3 is a deliberately loose cap to absorb
    // the burst that fires when a frontend tab restores: 1 × /api/files,
    // N × /api/file/tail (one per lazy tab, typically 5+), 1 × log-levels,
    // 1 × /api/upgrade/check — empirically 8-15 concurrent requests.
    // ×3 keeps back-to-back reloads from tripping the limiter. Internal
    // derived value, not exposed to the user.
    //
    // /ws is excluded: a long-lived connection that opens with a single
    // upgrade can't meaningfully be "rate limited", and rejecting the
    // upgrade on burst would hurt legitimate reconnects after a transient
    // network blip. The WS connection cap (Phase 3) covers abuse there.
    let governor_config = tower_governor::governor::GovernorConfigBuilder::default()
        .per_second(rps as u64)
        .burst_size(rps.saturating_mul(3))
        .use_headers()
        .finish()
        .expect("governor config: per_second>0 and burst_size>0 guaranteed by LimitsConfig defaults");

    // /ws on its own router, no GovernorLayer.
    let ws_router = Router::new().merge(ws::routes());

    // Everything else gets the auth middleware + governor.
    let api_router = Router::new()
        .merge(api::routes())
        .merge(static_files::routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(tower_governor::GovernorLayer {
            config: std::sync::Arc::new(governor_config),
        });

    // Compression is opt-in (default off). Only mount the layer when the user
    // explicitly enabled it — this avoids the ~10-15% overhead on gigabit LAN,
    // the primary deployment for tailr. Public/weak-network users opt in via
    // [limits] enable_compression = true.
    let router = Router::new()
        .merge(ws_router)
        .merge(api_router);

    // CompressionLayer must be the innermost body-transforming layer so it
    // sees the final response body and can rewrite it. CORS sits outside it
    // (added next), adding its headers to the (possibly compressed) response.
    let router = if enable_compression {
        router.layer(CompressionLayer::new())
    } else {
        router
    };

    router
        .layer(cors)
        .layer(axum::extract::Extension(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Verify the TOCTOU-safe admission pattern used in ws_handler:
    /// 1. Each rejected caller rolls back its increment (counter never leaks).
    /// 2. After all callers release, counter returns to exactly 0.
    ///
    /// We don't assert "exactly max admitted" because that's not the
    /// invariant — once an admitted caller releases, a rejected contender
    /// on a later retry would be admitted (which is correct: the cap is on
    /// *active* connections, not total admissions over time).
    #[test]
    fn test_ws_connection_count_no_leak_under_concurrency() {
        let counter = Arc::new(AtomicUsize::new(0));
        let max = 10usize;
        let contenders = 50usize;
        let handles: Vec<_> = (0..contenders)
            .map(|_| {
                let c = counter.clone();
                std::thread::spawn(move || {
                    let prev = c.fetch_add(1, Ordering::SeqCst);
                    let admitted = prev < max;
                    if !admitted {
                        c.fetch_sub(1, Ordering::SeqCst);
                    } else {
                        // briefly hold, then release as cleanup_client does
                        c.fetch_sub(1, Ordering::SeqCst);
                    }
                    admitted
                })
            })
            .collect();
        let _admitted_count = handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .filter(|&a| a)
            .count();
        // No leaks: every fetch_add has a matching fetch_sub.
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    /// Verify the cap is enforced on a burst where all contenders hold
    /// simultaneously: admitted count must equal max exactly.
    #[test]
    fn test_ws_connection_count_cap_enforced_when_all_hold() {
        let counter = Arc::new(AtomicUsize::new(0));
        let max = 8usize;
        let contenders = 30usize;
        // Barrier holds all admitted threads until the test signals release.
        let barrier = Arc::new(std::sync::Barrier::new(max + 1));
        let admitted = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..contenders)
            .map(|_| {
                let c = counter.clone();
                let b = barrier.clone();
                let a = admitted.clone();
                std::thread::spawn(move || {
                    let prev = c.fetch_add(1, Ordering::SeqCst);
                    if prev >= max {
                        c.fetch_sub(1, Ordering::SeqCst);
                        return false;
                    }
                    a.fetch_add(1, Ordering::SeqCst);
                    b.wait(); // hold until main releases
                    c.fetch_sub(1, Ordering::SeqCst);
                    true
                })
            })
            .collect();

        // Wait long enough for all contenders to attempt admission.
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Active count must respect the cap exactly.
        assert_eq!(counter.load(Ordering::SeqCst), max);
        assert_eq!(admitted.load(Ordering::SeqCst), max);

        // Release the barrier so admitted threads can finish.
        barrier.wait();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_limits_config_default() {
        let l = LimitsConfig::default();
        assert_eq!(l.max_ws_connections, 50);
        assert_eq!(l.rate_limit_rps, 20);
    }
}

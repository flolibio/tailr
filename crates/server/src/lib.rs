pub mod api;
pub(crate) mod error;
pub mod runtime;
pub mod static_files;
pub mod upgrade;
pub mod ws;

use arc_swap::ArcSwap;
use axum::extract::{Request, State};
use axum::http::{header, Method};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use dashmap::DashMap;
use tailr_core::limits::LimitsConfig;
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
    /// Runtime metrics sampler (sysinfo + TTL cache).
    /// Powers `GET /api/runtime`. No background thread — samples on demand only.
    pub runtime: Arc<runtime::RuntimeSampler>,
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

    error::ApiError::new(tailr_core::error::ErrorCode::Unauthorized).into_response()
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

    // Runtime sampler needs log_dirs to pick which disk to report (first
    // log_dir's mount point). Clone before `log_dirs` is moved into AppState.
    let runtime_sampler = Arc::new(runtime::RuntimeSampler::new(log_dirs.clone()));

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
        runtime: runtime_sampler,
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

    // Per-IP GCRA rate limit on the business REST/static surface.
    //
    // burst_size = rate_limit_rps * 10. GCRA's defining trait is slow recovery
    // after exhaustion (TAT-based penalty: a fully-drained bucket takes ~20s
    // before a single request passes again). For tailr this is the wrong
    // tradeoff — the "bursts" here are normal user behavior (tab restore fires
    // 8-15 concurrent requests; an impatient user may reload several times in
    // a few seconds), not abuse. A tight burst (×3 = 60) trips on ~6 rapid
    // reloads and then penalizes the user with a 20-60s throttle window.
    //
    // ×10 (= 200 at default rps=20) absorbs ~17 consecutive 12-request reloads
    // before exhausting — far beyond any realistic non-automated usage. This
    // sidesteps the slow-recovery problem entirely: normal use never drains the
    // bucket, so recovery speed is moot. Genuine abuse (sustained >200 req/s
    // from one IP) is still throttled; the frontend backoff-retries transient
    // edge-case 429s (see api.ts request()).
    //
    // /ws, /api/health, /api/runtime are excluded (separate routers): WS is a
    // long-lived single-upgrade connection where rate limiting is meaningless;
    // health/runtime are read-only TTL-cached status endpoints polled on a timer
    // that should not compete with business endpoints for quota.
    let governor_config = tower_governor::governor::GovernorConfigBuilder::default()
        .per_second(rps as u64)
        .burst_size(rps.saturating_mul(10))
        .use_headers()
        .finish()
        .expect("governor config: per_second>0 and burst_size>0 guaranteed by LimitsConfig defaults");

    // /ws on its own router, no GovernorLayer.
    let ws_router = Router::new().merge(ws::routes());

    // Read-only status endpoints (health, runtime) — exempt from governor.
    // Still auth-gated, just not rate-limited (see api::routes_unlimited).
    let status_router = Router::new()
        .merge(api::routes_unlimited())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

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
        .merge(status_router)
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

    // LimitsConfig tests live in `tailr-core` (crates/core/src/limits.rs) since
    // v0.12.0 moved the type there. These tests verified the same defaults and
    // validation rules that now have a single source of truth in core.
}

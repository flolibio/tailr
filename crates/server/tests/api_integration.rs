//! Integration tests for the API layer (REST handlers + error contract).
//!
//! Uses axum's `oneshot` to send requests through the real router in-process —
//! no TCP listener, no port binding. Tests focus on the public API contract
//! (status codes, error body shape, success envelope) rather than internal logic,
//! locking the v1.0 surface before the freeze.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use arc_swap::ArcSwap;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use dashmap::DashMap;
use tokio::sync::Mutex;
use tower::ServiceExt; // for `oneshot`

use tailr_core::limits::LimitsConfig;
use tailr_core::runtime::RuntimeSampler;
use tailr_protocol::{LogLevelConfig, LogTimezone};
use tailr_search_engine::LevelDetector;
use tailr_server::{api, upgrade, AppState};
use tailr_server::app as full_app;

/// Construct a minimal AppState for testing. All fields are populated with
/// sane defaults; the `log_dirs` point at a tempdir so file operations work.
fn make_state(log_dirs: Vec<PathBuf>, config_path: PathBuf, token: String) -> Arc<AppState> {
    let level_config = LogLevelConfig {
        preset: "general".to_string(),
        levels: vec![],
    };
    Arc::new(AppState {
        watcher: Arc::new(Mutex::new(
            tailr_tail_engine::FileWatcher::new(
                std::time::Duration::from_millis(100),
                Arc::new(ArcSwap::from_pointee(LevelDetector::from_config(&level_config))),
                Arc::new(LogTimezone::default()),
            )
            .expect("failed to create FileWatcher"),
        )),
        line_indices: DashMap::new(),
        file_subscribers: Mutex::new(HashMap::new()),
        ws_clients: Mutex::new(HashMap::new()),
        ws_connection_count: AtomicUsize::new(0),
        log_dirs: log_dirs.clone(),
        log_files: vec![],
        start_time: Instant::now(),
        level_config: Arc::new(ArcSwap::from_pointee(level_config)),
        level_detector: Arc::new(ArcSwap::from_pointee(LevelDetector::from_config(&LogLevelConfig {
            preset: "general".to_string(),
            levels: vec![],
        }))),
        config_path,
        token,
        allowed_dirs: log_dirs,
        log_timezone: Arc::new(LogTimezone::default()),
        upgrade_service: Arc::new(upgrade::UpgradeService::new()),
        runtime: Arc::new(RuntimeSampler::new(vec![])),
        limits: LimitsConfig::default(),
    })
}

/// Build a test router from just the API routes (no auth middleware, no governor,
/// no background loops). This tests handler logic + error mapping directly.
fn test_router(state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .merge(api::routes())
        .merge(api::routes_unlimited())
        .layer(axum::extract::Extension(state))
}

/// Helper: send a GET request through the router and return (status, body_json).
async fn get_json(
    router: &axum::Router,
    uri: &str,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ── Health endpoint ──────────────────────────────────────────

#[tokio::test]
async fn health_returns_200_with_status_version_uptime() {
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["status"], "ok");
    assert!(json["data"]["version"].is_string());
    assert!(json["data"]["uptimeSeconds"].is_number());
}

// ── File listing: path not found ─────────────────────────────

#[tokio::test]
async fn list_files_nonexistent_path_returns_404_with_error_body() {
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(
        vec![tmp.path().to_path_buf()],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/files?path=/nonexistent/path/xyz").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "NOT_FOUND");
    assert!(json["error"]["message"].is_string());
}

// ── File listing: path outside allowed dirs ──────────────────

#[tokio::test]
async fn list_files_path_outside_log_dirs_returns_403() {
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(
        vec![tmp.path().to_path_buf()],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    // /etc exists but is outside the tempdir log_dir → PATH_NOT_ALLOWED → 403.
    let (status, json) = get_json(&router, "/api/files?path=/etc").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "PATH_NOT_ALLOWED");
}

// ── File listing: success ────────────────────────────────────

#[tokio::test]
async fn list_files_root_returns_entries() {
    let tmp = tempfile::tempdir().unwrap();
    // Create a test file in the log dir.
    std::fs::write(tmp.path().join("app.log"), "line1\nline2\n").unwrap();
    let state = make_state(
        vec![tmp.path().to_path_buf()],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/files").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert!(json["data"]["entries"].is_array());
}

// ── File tail: nonexistent file ──────────────────────────────

#[tokio::test]
async fn file_tail_nonexistent_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(
        vec![tmp.path().to_path_buf()],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/file/tail?path=/nonexistent/file.log").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "NOT_FOUND");
}

// ── File tail: success ───────────────────────────────────────

#[tokio::test]
async fn file_tail_existing_file_returns_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let log_file = tmp.path().join("app.log");
    std::fs::write(&log_file, "2026-01-01 12:00:00 INFO hello\n").unwrap();
    // Canonicalize the log dir so it matches validate_path's canonicalize()
    // (macOS tempdirs are under /private/var, a symlink of /var).
    let canonical_dir = tmp.path().canonicalize().unwrap();
    let state = make_state(
        vec![canonical_dir],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let path_str = log_file.to_string_lossy().to_string();
    let uri = format!("/api/file/tail?path={}&lines=10", urlencoding::encode(&path_str));
    let (status, json) = get_json(&router, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert!(json["data"]["totalLines"].as_u64().unwrap_or(0) >= 1);
}

// ── Log levels: GET ──────────────────────────────────────────

#[tokio::test]
async fn get_log_levels_returns_current_config() {
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/config/log-levels").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["preset"], "general");
    assert!(json["data"]["levels"].is_array());
}

// ── Log levels: POST missing CSRF when token set ─────────────

#[tokio::test]
async fn save_log_levels_missing_csrf_with_token_returns_403() {
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(
        vec![],
        tmp.path().join("config.toml"),
        "secret-token".to_string(), // token set → CSRF required
    );
    let router = test_router(state);

    // POST without X-Requested-With header.
    let body = r#"{"preset":"general","levels":[]}"#;
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config/log-levels")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ── Log levels: POST applies to runtime, doesn't touch config.toml ──

#[tokio::test]
async fn save_log_levels_applies_runtime_and_preserves_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.toml");

    // Write a config file with comments — the bug was that save_log_levels
    // rewrote the file via toml round-trip, silently stripping all comments.
    let original_content = r#"# This is a user comment
# Another comment line
bind = "0.0.0.0:7700"

[limits]
max_ws_connections = 50
"#;
    std::fs::write(&config_path, original_content).unwrap();

    let state = make_state(vec![], config_path.clone(), String::new());
    let router = test_router(state);

    // POST a new log levels config.
    let body = r##"{"preset":"custom","levels":[{"name":"ERROR","keywords":["ERROR"],"colorLight":"#A32D2D","colorDark":"#F09595"}]}"##;
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config/log-levels")
                .header("Content-Type", "application/json")
                .header("X-Requested-With", "XMLHttpRequest")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The config file must be untouched — comments preserved, no round-trip.
    let after = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(
        after, original_content,
        "config.toml was modified by save_log_levels — it must not be written to"
    );
    assert!(
        after.contains("# This is a user comment"),
        "user comments were stripped from config.toml"
    );
}

#[tokio::test]
async fn save_log_levels_updates_get_response() {
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    // POST a custom config.
    let body = r##"{"preset":"custom","levels":[{"name":"FATAL","keywords":["FATAL"],"colorLight":"#CC2D26","colorDark":"#FF6B63"}]}"##;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config/log-levels")
                .header("Content-Type", "application/json")
                .header("X-Requested-With", "XMLHttpRequest")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // GET should now return the updated config (runtime hot-reload verified).
    let (status, json) = get_json(&router, "/api/config/log-levels").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["preset"], "custom");
    assert_eq!(json["data"]["levels"][0]["name"], "FATAL");
}

#[tokio::test]
async fn perform_upgrade_without_token_passes_csrf_gate() {
    // No token set → auth is globally disabled (auth_middleware passes through).
    // The upgrade endpoint no longer forces a token; the only hard gate is the
    // X-Requested-With CSRF header. With the header present, the request must
    // reach the service layer (not be rejected at 403).
    let state = make_state(
        vec![],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(), // no token → no forced-auth rejection (v1.0.2+)
    );
    let router = test_router(state);

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/upgrade")
                .header("X-Requested-With", "XMLHttpRequest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The request was NOT rejected by the token gate. On a test machine the
    // platform is typically unsupported (macOS) → 400 UNSUPPORTED_PLATFORM, or
    // the detached task returns 200 "started". Either way it must NOT be 403
    // (which would mean the old TOKEN_REQUIRED / CSRF gate fired).
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "upgrade without token must not be rejected by a forced-auth gate"
    );
}

#[tokio::test]
async fn perform_upgrade_without_csrf_header_returns_403() {
    // CSRF header is the unconditional hard gate on the upgrade endpoint.
    // Without X-Requested-With, the request is rejected regardless of token.
    let state = make_state(
        vec![],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/upgrade")
                // No X-Requested-With header → CSRF check fails.
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "FORBIDDEN");
}

// ── Upgrade check: success (may be cached/empty) ─────────────

#[tokio::test]
async fn check_upgrade_returns_200_or_500_but_never_200_success_false() {
    // The upgrade check may fail (no network in CI) but it must never return
    // HTTP 200 + {success:false} — that pattern is eliminated. A failure is
    // HTTP 500 + error body.
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/upgrade/check").await;

    // Either success (200 + data) or internal error (500 + error body).
    if status == StatusCode::OK {
        assert_eq!(json["success"], true);
        assert!(json["data"]["currentVersion"].is_string());
    } else {
        assert_eq!(json["success"], false);
        assert!(json["error"]["code"].is_string());
        // Must NOT be the old "HTTP 200 + success:false" pattern.
        assert_ne!(status, StatusCode::OK);
    }
}

// ── Runtime endpoint ─────────────────────────────────────────

#[tokio::test]
async fn runtime_returns_200_with_metrics() {
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/runtime").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert!(json["data"]["systemTotalMemoryBytes"].as_u64().unwrap_or(0) > 0);
    assert!(json["data"]["uptimeSeconds"].as_u64().unwrap_or(0) < 3600); // < 1h (test runs fast)
}

// ── OpenAPI spec endpoint ────────────────────────────────────

#[tokio::test]
async fn openapi_spec_endpoint_returns_valid_json() {
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/docs/openapi.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["openapi"], "3.1.0");
    assert!(json["paths"].is_object());
    assert!(json["paths"]["/api/health"].is_object());
    assert!(json["paths"]["/api/files"].is_object());
    assert!(json["components"]["schemas"].is_object());
}

// ── Error body shape contract (frozen at v1.0) ───────────────

#[tokio::test]
async fn error_body_shape_is_success_false_error_code_message() {
    // Verify the exact shape: {success:false, error:{code:"...", message:"..."}}
    // This is the v1.0 frozen contract — any change here is breaking.
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(
        vec![tmp.path().to_path_buf()],
        PathBuf::from("/tmp/nonexistent.toml"),
        String::new(),
    );
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/files?path=/nonexistent/xyz").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    // Exact field names (camelCase).
    assert_eq!(json["success"], false);
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
    // No stray fields.
    let error_obj = json["error"].as_object().unwrap();
    assert_eq!(error_obj.len(), 2);
    assert!(error_obj.contains_key("code"));
    assert!(error_obj.contains_key("message"));
}

// ── Success body shape contract ──────────────────────────────

#[tokio::test]
async fn success_body_shape_is_success_true_data() {
    // Verify the exact shape: {success:true, data:<T>}
    let state = make_state(vec![], PathBuf::from("/tmp/nonexistent.toml"), String::new());
    let router = test_router(state);

    let (status, json) = get_json(&router, "/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert!(json["data"].is_object());
    // No error field on success.
    assert!(json.get("error").is_none() || json["error"].is_null());
}

// ── WebSocket auth: regression for WS auth bypass ───────────────
//
// Before the fix, the /ws router was assembled without the auth middleware,
// so a configured TAILR_TOKEN protected every REST endpoint but left the live
// log stream open. These tests build the REAL `app()` router and assert that
// /ws now goes through `auth_middleware`:
//   - no token + token set        → 401 (blocked before upgrade)
//   - wrong token                 → 401
//   - correct token, URL-encoded  → not 401 (auth passes; upgrade layer then
//                                            decides based on WS headers)
//
// We assert "not 401" rather than "101 Switching Protocols" because a bare
// oneshot GET without Upgrade/Connection headers won't complete the handshake;
// the point here is only to prove auth no longer bypasses /ws.

/// Build the production router via `app()`. Spawns detached background loops
/// (watcher, upgrade check); those are silent in tests and die with the test
/// runtime. `token` controls whether auth is enforced.
fn real_app(token: &str) -> axum::Router {
    full_app(
        vec![],
        PathBuf::from("/tmp/nonexistent.toml"),
        LogLevelConfig {
            preset: "general".to_string(),
            levels: vec![],
        },
        LogTimezone::default(),
        token.to_string(),
        LimitsConfig::default(),
    )
}

/// Send a GET through the router and return just the status code.
async fn get_status(router: &axum::Router, uri: &str) -> StatusCode {
    router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn ws_without_token_is_rejected_when_token_set() {
    let router = real_app("s3cret-token");
    // No token query param → auth_middleware must block before the upgrade.
    let status = get_status(&router, "/ws").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "WS without token must be 401 when TAILR_TOKEN is set (auth bypass regression)"
    );
}

#[tokio::test]
async fn ws_with_wrong_token_is_rejected() {
    let router = real_app("s3cret-token");
    let status = get_status(&router, "/ws?token=wrong").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "WS with wrong token must be 401"
    );
}

#[tokio::test]
async fn ws_with_correct_alphanumeric_token_passes_auth() {
    let router = real_app("s3cret-token");
    let status = get_status(&router, "/ws?token=s3cret-token").await;
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "WS with correct token must pass the auth layer"
    );
}

#[tokio::test]
async fn ws_with_url_encoded_token_passes_auth() {
    // Token contains a space and a '+' — characters the frontend
    // encodeURIComponent encodes. Before the percent-decode fix, the raw
    // query string never matched state.token.
    let raw_token = "my token+key";
    let encoded = urlencoding::encode(raw_token);
    let router = real_app(raw_token);
    let uri = format!("/ws?token={}", encoded);
    let status = get_status(&router, &uri).await;
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "WS with percent-encoded token must pass auth (decode regression)"
    );
}

#[tokio::test]
async fn ws_without_token_allowed_when_no_token_set() {
    // When TAILR_TOKEN is empty, auth is globally disabled — WS must connect
    // freely (no regression of the open-by-default mode).
    let router = real_app("");
    let status = get_status(&router, "/ws").await;
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "WS must not require a token when none is configured"
    );
}

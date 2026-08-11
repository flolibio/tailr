use axum::extract::{Extension, Query};
use axum::http::HeaderMap;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use tailr_core::error::{CoreError, ErrorCode};
use tailr_protocol::{try_parse_timestamp, LogEntry, LogLevelConfig, LogTimezone};
use tailr_search_engine::LevelDetector;
use tailr_tail_engine::LineIndex;

use crate::error::{ApiError, ApiSuccess};
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct FileEntry {
    name: String,
    path: String,
    size: u64,
    modified: Option<String>,
    is_dir: bool,
    /// Nested children for directories, populated when a recursive depth was
    /// requested (`?depth=N`). Empty for files or when not recursing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(no_recursion)]
    children: Vec<FileEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct FileListData {
    entries: Vec<FileEntry>,
}

#[derive(Deserialize)]
struct FileListParams {
    path: Option<String>,
    /// Recurse into subdirectories up to this many levels (default 1 = flat).
    /// Hard-capped at `MAX_LIST_DEPTH` to protect against pathological trees.
    #[serde(default)]
    depth: Option<u32>,
}

/// Hard cap on recursive depth for file listing. Prevents a misconfigured
/// `log_dir` (e.g. pointing at `/`) from enumerating the whole filesystem.
const MAX_LIST_DEPTH: u32 = 4;
/// Hard cap on total entries returned by a single recursive listing, to bound
/// latency and payload size on huge directory trees.
const MAX_LIST_ENTRIES: usize = 5000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct FileTailData {
    entries: Vec<LogEntry>,
    total_lines: u64,
}

#[derive(Deserialize)]
struct FileTailParams {
    path: String,
    #[serde(default)]
    lines: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct HealthData {
    status: String,
    version: String,
    uptime_seconds: u64,
    /// True while a detached upgrade task is running (download → replace →
    /// restart). Frontend polls this to show a spinner and disable the upgrade
    /// button — survives page refresh because the flag lives in the process.
    upgrade_in_progress: bool,
}

/// Runtime metrics snapshot returned by `GET /api/runtime`.
/// Combines sysinfo-derived fields (cpu/mem/disk) with cheap AppState reads
/// (ws connections, uptime). All values are instantaneous.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct RuntimeData {
    process_memory_bytes: u64,
    process_cpu_percent: f32,
    system_total_memory_bytes: u64,
    system_used_memory_bytes: u64,
    system_cpu_percent: f32,
    disk_total_bytes: u64,
    disk_used_bytes: u64,
    ws_connections: usize,
    uptime_seconds: u64,
}

#[derive(Deserialize)]
struct UpgradeCheckParams {
    /// Bypass cache and force a fresh GitHub query.
    #[serde(default)]
    force: Option<bool>,
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/files", get(list_files))
        .route("/api/file/tail", get(file_tail))
        .route("/api/config/log-levels", get(get_log_levels).post(save_log_levels))
        .route("/api/upgrade/check", get(check_upgrade))
        .route("/api/upgrade", axum::routing::post(perform_upgrade))
}

/// Read-only status endpoints exempt from the governor rate limiter.
///
/// `/api/health` and `/api/runtime` are lightweight, TTL-cached, carry no
/// side effects, and are polled on a timer (health by LB probes / the About
/// panel; runtime by the Runtime panel at 5s). Letting them share the same
/// per-IP GCRA bucket as business endpoints (file tail, log levels) means a
/// sustained runtime poll slowly eats into the client's request budget and,
/// combined with other traffic, eventually trips 429. Since these endpoints
/// are cheap and non-mutating, they don't need the same abuse protection as
/// the business surface — exempting them keeps the panel's 5s poll from
/// competing with real work for quota.
pub fn routes_unlimited() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runtime", get(runtime))
        .route(
            "/api/docs/openapi.json",
            get(|| async {
                use utoipa::OpenApi as _;
                axum::Json(crate::openapi::ApiDoc::openapi())
            }),
        )
}

pub(crate) fn validate_path(
    requested: &str,
    allowed_dirs: &[PathBuf],
    allowed_files: &[PathBuf],
) -> Result<PathBuf, ErrorCode> {
    let path = PathBuf::from(requested);
    let canonical = path.canonicalize().map_err(|_| ErrorCode::NotFound)?;

    let is_allowed = allowed_dirs.iter().any(|d| canonical.starts_with(d))
        || allowed_files.contains(&canonical);

    if is_allowed {
        Ok(canonical)
    } else {
        Err(ErrorCode::PathNotAllowed)
    }
}

/// List log files and directories. Returns entries under the configured
/// `log` paths, or under a specific subdirectory when `?path=` is given.
/// Recursive depth is controlled by `?depth=` (default 1, max 4).
#[utoipa::path(
    get,
    path = "/api/files",
    params(
        ("path" = Option<String>, Query, description = "Subdirectory path to list (default: configured log roots)"),
        ("depth" = Option<u32>, Query, description = "Recursive depth (default 1 = flat, max 4)"),
    ),
    responses(
        (status = 200, description = "File listing", body = FileListData),
        (status = 404, description = "Directory not found", body = crate::error::ErrorBody),
        (status = 403, description = "Path not allowed", body = crate::error::ErrorBody),
        (status = 500, description = "Internal error", body = crate::error::ErrorBody),
    ),
    tag = "files",
)]
async fn list_files(
    Query(params): Query<FileListParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ApiSuccess<FileListData>>, ApiError> {
    let mut entries: Vec<FileEntry> = Vec::new();
    // Resolve requested depth, clamped to the hard cap. Default 1 (flat listing).
    let depth = params.depth.unwrap_or(1).clamp(1, MAX_LIST_DEPTH);
    let mut total: usize = 0;

    match params.path {
        Some(p) => {
            let dir = validate_path(&p, &state.allowed_dirs, &state.log_files)?;
            if let Err(e) = read_dir_entries(&dir, &mut entries, depth, &mut total).await {
                tracing::error!("failed to read directory {:?}: {}", dir, e);
                return Err(ErrorCode::Internal.into());
            }
        }
        None => {
            for file in &state.log_files {
                if file.exists() && file.is_file() {
                    let metadata = tokio::fs::metadata(file).await.ok();
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified = metadata
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            let dt: chrono::DateTime<chrono::Utc> = t.into();
                            dt.to_rfc3339()
                        });
                    entries.push(FileEntry {
                        name: file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string()),
                        path: file.to_string_lossy().to_string(),
                        size,
                        modified,
                        is_dir: false,
                        children: Vec::new(),
                    });
                }
            }

            for dir in &state.log_dirs {
                if dir.exists() && dir.is_dir() {
                    // Recurse into each configured log_dir so its subtree is
                    // pre-fetched (depth levels). Without this, multi-log_dir
                    // roots only listed the dir name with empty children, so the
                    // frontend's preload/search/expand-on-click all broke.
                    let mut sub_entries = Vec::new();
                    if let Err(e) = read_dir_entries(
                        dir,
                        &mut sub_entries,
                        depth,
                        &mut total,
                    )
                    .await
                    {
                        tracing::error!("failed to read directory {:?}: {}", dir, e);
                        return Err(ErrorCode::Internal.into());
                    }
                    // Preserve the log_dir's own name/path as the parent node,
                    // attaching the recursed children.
                    entries.push(FileEntry {
                        name: dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| dir.display().to_string()),
                        path: dir.to_string_lossy().to_string(),
                        size: 0,
                        modified: None,
                        is_dir: true,
                        children: sub_entries,
                    });
                }
            }
            if state.log_dirs.len() == 1 && state.log_files.is_empty() {
                entries.clear();
                if let Err(e) =
                    read_dir_entries(&state.log_dirs[0], &mut entries, depth, &mut total).await
                {
                    tracing::error!("failed to read directory {:?}: {}", state.log_dirs[0], e);
                    return Err(ErrorCode::Internal.into());
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Json(ApiSuccess::ok(FileListData { entries })))
}

async fn read_dir_entries(
    dir: &std::path::Path,
    entries: &mut Vec<FileEntry>,
    remaining_depth: u32,
    total: &mut usize,
) -> std::io::Result<()> {
    read_dir_entries_inner(dir, entries, remaining_depth, total).await
}

/// Boxed-inner form so the async fn can recurse (a recursive async fn needs
/// indirection to have a finite future size). Mirrors the dir_has_text_files_inner
/// pattern already used in this file.
fn read_dir_entries_inner<'a>(
    dir: &'a std::path::Path,
    entries: &'a mut Vec<FileEntry>,
    remaining_depth: u32,
    total: &'a mut usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut read_dir = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            // Stop if we've hit the global entry cap — protects against huge trees.
            if *total >= MAX_LIST_ENTRIES {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let metadata = entry.metadata().await.ok();
            let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

            // Skip dirs with no text files anywhere (prunes barren branches),
            // and skip non-text files at every level.
            if is_dir {
                if !dir_has_text_files(&entry.path()).await {
                    continue;
                }
            } else if !is_text_file(&entry.path(), &name).await {
                continue;
            }

            let modified = metadata
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: DateTime<Utc> = t.into();
                    dt.to_rfc3339()
                });

            // Recurse into subdirectories while depth remains, collecting children.
            let children = if is_dir && remaining_depth > 1 {
                let mut child_entries = Vec::new();
                read_dir_entries_inner(&entry.path(), &mut child_entries, remaining_depth - 1, total).await?;
                child_entries
            } else {
                Vec::new()
            };

            *total += 1;
            entries.push(FileEntry {
                name,
                path: entry.path().to_string_lossy().to_string(),
                size,
                modified,
                is_dir,
                children,
            });
        }
        Ok(())
    })
}

async fn dir_has_text_files(dir: &std::path::Path) -> bool {
    dir_has_text_files_inner(dir, 0).await
}

fn dir_has_text_files_inner(dir: &std::path::Path, depth: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
    Box::pin(async move {
        if depth > 2 {
            return true;
        }
        let mut read_dir = match tokio::fs::read_dir(dir).await {
            Ok(r) => r,
            Err(_) => return false,
        };
        while let Some(entry) = read_dir.next_entry().await.unwrap_or(None) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = entry.metadata().await.map(|m| m.is_dir()).unwrap_or(false);
            if is_dir {
                if dir_has_text_files_inner(&entry.path(), depth + 1).await {
                    return true;
                }
            } else if is_text_file(&entry.path(), &name).await {
                return true;
            }
        }
        false
    })
}

async fn is_text_file(path: &std::path::Path, _name: &str) -> bool {
    let text_extensions: &[&str] = &[
        "log", "txt", "text", "out", "err", "stdout", "stderr",
        "json", "xml", "yaml", "yml", "toml", "ini", "conf", "cfg",
        "csv", "tsv", "md", "rst",
        "py", "rb", "js", "ts", "go", "rs", "java", "c", "cpp", "h", "hpp",
        "sh", "bash", "zsh", "fish",
        "sql", "html", "css", "scss",
        "bak", "old", "prev", "save",
    ];

    let binary_extensions: &[&str] = &[
        "exe", "dll", "so", "dylib", "bin", "dat", "db", "sqlite",
        "zip", "gz", "tar", "bz2", "xz", "7z", "rar",
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "webp",
        "mp3", "mp4", "avi", "mkv", "mov", "wav", "flac",
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
        "woff", "woff2", "ttf", "otf", "eot",
        "pyc", "pyo", "class", "o", "obj",
    ];

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if text_extensions.iter().any(|e| e.eq_ignore_ascii_case(&lower)) {
            return true;
        }
        if binary_extensions.iter().any(|e| e.eq_ignore_ascii_case(&lower)) {
            return false;
        }
        return is_likely_text(path).await;
    }

    is_likely_text(path).await
}

async fn is_likely_text(path: &std::path::Path) -> bool {
    use tokio::io::AsyncReadExt;
    let mut file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = [0u8; 512];
    let n = match file.read(&mut buf).await {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n == 0 {
        return true;
    }
    !buf[..n].contains(&0)
}

/// Get the last N lines of a log file (default 200, max 5000).
#[utoipa::path(
    get,
    path = "/api/file/tail",
    params(
        ("path" = String, Query, description = "Absolute path to the log file"),
        ("lines" = Option<u64>, Query, description = "Number of lines to read from the end (default 200, max 5000)"),
    ),
    responses(
        (status = 200, description = "Tail entries", body = FileTailData),
        (status = 404, description = "File not found", body = crate::error::ErrorBody),
        (status = 403, description = "Path not allowed", body = crate::error::ErrorBody),
    ),
    tag = "files",
)]
async fn file_tail(
    Query(params): Query<FileTailParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ApiSuccess<FileTailData>>, ApiError> {
    let path = validate_path(&params.path, &state.allowed_dirs, &state.log_files)?;

    let lines = params.lines.unwrap_or(200).min(5000) as usize;

    let tail = {
        let p = path.clone();
        match tokio::task::spawn_blocking(move || LineIndex::tail_start(&p, lines)).await {
            Ok(Ok(tail)) => tail,
            _ => {
                return Ok(Json(ApiSuccess::ok(FileTailData {
                    entries: Vec::new(),
                    total_lines: 0,
                })))
            }
        }
    };

    if tail.total_lines == 0 {
        return Ok(Json(ApiSuccess::ok(FileTailData {
            entries: Vec::new(),
            total_lines: 0,
        })));
    }

    let start_line = tail.total_lines.saturating_sub(lines as u64);
    let detector = state.level_detector.load();
    let entries = read_lines_from(
        &path,
        tail.start_byte,
        lines,
        start_line,
        &detector,
        &state.log_timezone,
    )
    .await;

    Ok(Json(ApiSuccess::ok(FileTailData {
        entries,
        total_lines: tail.total_lines,
    })))
}

/// Server health check (read-only, exempt from rate limiting).
#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Server health", body = HealthData),
    ),
    tag = "system",
)]
async fn health(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiSuccess<HealthData>> {
    Json(ApiSuccess::ok(HealthData {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        upgrade_in_progress: state.upgrade_service.is_upgrade_in_progress(),
    }))
}

/// `GET /api/runtime` — runtime resource snapshot (CPU / memory / disk / WS
/// connections / uptime). Sampling is TTL-cached (5s) and refresh runs in
/// `spawn_blocking` so it never stalls tokio workers. Read-only, no CSRF.
/// Runtime resource snapshot: process/system CPU+memory, disk, WS connections,
/// uptime (read-only, exempt from rate limiting, TTL-cached 5s).
#[utoipa::path(
    get,
    path = "/api/runtime",
    responses(
        (status = 200, description = "Runtime metrics", body = RuntimeData),
    ),
    tag = "system",
)]
async fn runtime(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiSuccess<RuntimeData>> {
    use std::sync::atomic::Ordering;
    // Core's `sample_blocking` is synchronous; offload the (potentially
    // blocking) sysinfo refresh to the tokio blocking pool.
    let sampler = state.runtime.clone();
    let ws = state.ws_connection_count.load(Ordering::SeqCst);
    let uptime = state.start_time.elapsed().as_secs();
    let (snap, ws_connections, uptime_seconds) =
        tokio::task::spawn_blocking(move || sampler.sample_blocking(ws, uptime))
            .await
            .unwrap_or_else(|e| {
                tracing::error!("runtime sample task failed: {e}; returning zero snapshot");
                (crate::runtime::RuntimeSnapshot::default(), ws, uptime)
            });
    Json(ApiSuccess::ok(RuntimeData {
        process_memory_bytes: snap.process_memory_bytes,
        process_cpu_percent: snap.process_cpu_percent,
        system_total_memory_bytes: snap.system_total_memory_bytes,
        system_used_memory_bytes: snap.system_used_memory_bytes,
        system_cpu_percent: snap.system_cpu_percent,
        disk_total_bytes: snap.disk_total_bytes,
        disk_used_bytes: snap.disk_used_bytes,
        ws_connections,
        uptime_seconds,
    }))
}

/// Check for a newer release. Read-only — no CSRF/auth gating beyond the global
/// middleware (token still required if set, but the endpoint carries no sensitive
/// data and never mutates). Serves from cache unless `?force=true`.
/// Check for a newer release on GitHub (read-only). Serves from cache unless
/// `?force=true`. Returns version info + platform support flag.
#[utoipa::path(
    get,
    path = "/api/upgrade/check",
    params(
        ("force" = Option<bool>, Query, description = "Bypass cache and force a fresh GitHub query"),
    ),
    responses(
        (status = 200, description = "Update info", body = crate::upgrade::UpdateInfo),
        (status = 500, description = "Check failed", body = crate::error::ErrorBody),
    ),
    tag = "upgrade",
)]
async fn check_upgrade(
    Query(params): Query<UpgradeCheckParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ApiSuccess<crate::upgrade::UpdateInfo>>, ApiError> {
    match state.upgrade_service.check_update(params.force.unwrap_or(false)).await {
        Ok(info) => Ok(Json(ApiSuccess::ok(info))),
        Err(e) => {
            tracing::error!("failed to check update: {}", e);
            Err(CoreError::with_detail(ErrorCode::Internal, e).into())
        }
    }
}

/// Perform the upgrade: download + replace binary + delegate restart.
///
/// Auth follows the global policy (same as every other endpoint): when a token
/// is configured, `auth_middleware` enforces Bearer auth; when no token is set,
/// auth is disabled and the endpoint is reachable without credentials. The
/// `X-Requested-With` CSRF header is required unconditionally (defense against
/// cross-site forgery from a logged-in browser context).
#[utoipa::path(
    post,
    path = "/api/upgrade",
    responses(
        (status = 200, description = "Upgrade result", body = crate::upgrade::UpgradeResult),
        (status = 400, description = "Unsupported platform", body = crate::error::ErrorBody),
        (status = 403, description = "CSRF check failed", body = crate::error::ErrorBody),
        (status = 409, description = "Upgrade already in progress", body = crate::error::ErrorBody),
        (status = 500, description = "Upgrade failed", body = crate::error::ErrorBody),
    ),
    tag = "upgrade",
    security(("bearerAuth" = [])),
)]
async fn perform_upgrade(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiSuccess<crate::upgrade::UpgradeResult>>, ApiError> {
    // CSRF header required unconditionally (the only hard gate on this endpoint).
    if headers.get("X-Requested-With").is_none() {
        return Err(ErrorCode::Forbidden.into());
    }

    match state.upgrade_service.perform_upgrade().await {
        Ok(result) => Ok(Json(ApiSuccess::ok(result))),
        Err(e) => {
            tracing::error!("upgrade failed: {}", e);
            Err(upgrade_err_to_api(&e))
        }
    }
}

/// Get the current log level configuration (preset + level definitions).
#[utoipa::path(
    get,
    path = "/api/config/log-levels",
    responses(
        (status = 200, description = "Log level config", body = LogLevelConfig),
    ),
    tag = "config",
)]
async fn get_log_levels(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiSuccess<LogLevelConfig>> {
    let config = state.level_config.load();
    Json(ApiSuccess::ok(config.as_ref().clone()))
}

/// Save the log level configuration to config.toml and update the live detector.
/// Requires CSRF header (`X-Requested-With`) when token auth is enabled.
#[utoipa::path(
    post,
    path = "/api/config/log-levels",
    request_body = LogLevelConfig,
    responses(
        (status = 200, description = "Saved config", body = LogLevelConfig),
        (status = 400, description = "Invalid config", body = crate::error::ErrorBody),
        (status = 403, description = "CSRF check failed", body = crate::error::ErrorBody),
        (status = 500, description = "Config write failed", body = crate::error::ErrorBody),
    ),
    tag = "config",
    security(("bearerAuth" = [])),
)]
async fn save_log_levels(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Json(new_config): Json<LogLevelConfig>,
) -> Result<Json<ApiSuccess<LogLevelConfig>>, ApiError> {
    if !state.token.is_empty() && headers.get("X-Requested-With").is_none() {
        return Err(ErrorCode::Forbidden.into());
    }

    // 热更新运行时 detector + 内存配置。不持久化到 config.toml
    // （config.toml 是人工维护的部署配置，程序回写会丢失注释）。
    // 永久修改请编辑 config.toml 的 [log_levels] 后重启。
    let new_detector = LevelDetector::from_config(&new_config);
    state.level_detector.store(Arc::new(new_detector));
    state.level_config.store(Arc::new(new_config.clone()));

    Ok(Json(ApiSuccess::ok(new_config)))
}

/// Map an upgrade error string (from core's `UpgradeEngine` / Web's
/// `UpgradeService`) to an [`ApiError`]. The engine returns SCREAMING_SNAKE
/// codes for known domain errors; everything else (network failures, etc.) maps
/// to `Internal`.
fn upgrade_err_to_api(e: &str) -> ApiError {
    let code = match e {
        "UNSUPPORTED_PLATFORM" => ErrorCode::UnsupportedPlatform,
        "PERMISSION_DENIED" => ErrorCode::PermissionDenied,
        "UPGRADE_IN_PROGRESS" => ErrorCode::UpgradeInProgress,
        _ => ErrorCode::Internal,
    };
    CoreError::with_detail(code, e).into()
}

async fn read_lines_from(
    path: &PathBuf,
    start_byte: u64,
    max_lines: usize,
    base_line: u64,
    detector: &tailr_search_engine::LevelDetector,
    log_timezone: &LogTimezone,
) -> Vec<LogEntry> {
    use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};

    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut reader = BufReader::new(file);
    if reader.seek(SeekFrom::Start(start_byte)).await.is_err() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut buf = String::new();
    let mut line_num: u64 = base_line;

    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf).await {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }

        let trimmed = buf.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed.is_empty() {
            line_num += 1;
            continue;
        }

        let level = detector.detect(trimmed);
        let (timestamp, raw_timestamp) = try_parse_timestamp(trimmed, log_timezone);

        entries.push(LogEntry {
            line_num,
            raw: trimmed.to_string(),
            level,
            timestamp,
            raw_timestamp,
            fields: None,
        });

        line_num += 1;
        if entries.len() >= max_lines {
            break;
        }
    }

    entries
}

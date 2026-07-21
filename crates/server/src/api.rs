use axum::extract::{Extension, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use tailr_protocol::{try_parse_timestamp, LogEntry, LogLevelConfig, LogTimezone};
use tailr_search_engine::LevelDetector;
use tailr_tail_engine::LineIndex;

use crate::AppState;

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileEntry {
    name: String,
    path: String,
    size: u64,
    modified: Option<String>,
    is_dir: bool,
    /// Nested children for directories, populated when a recursive depth was
    /// requested (`?depth=N`). Empty for files or when not recursing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<FileEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileListData {
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
struct FileTailData {
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
struct HealthData {
    status: String,
    version: String,
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
        .route("/api/health", get(health))
        .route("/api/config/log-levels", get(get_log_levels).post(save_log_levels))
        .route("/api/upgrade/check", get(check_upgrade))
        .route("/api/upgrade", axum::routing::post(perform_upgrade))
}

pub(crate) fn validate_path(
    requested: &str,
    allowed_dirs: &[PathBuf],
    allowed_files: &[PathBuf],
) -> Result<PathBuf, StatusCode> {
    let path = PathBuf::from(requested);
    let canonical = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;

    let is_allowed = allowed_dirs.iter().any(|d| canonical.starts_with(d))
        || allowed_files.contains(&canonical);

    if is_allowed {
        Ok(canonical)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn list_files(
    Query(params): Query<FileListParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<FileListData>> {
    let mut entries: Vec<FileEntry> = Vec::new();
    // Resolve requested depth, clamped to the hard cap. Default 1 (flat listing).
    let depth = params.depth.unwrap_or(1).clamp(1, MAX_LIST_DEPTH);
    let mut total: usize = 0;

    match params.path {
        Some(p) => {
            let dir = match validate_path(&p, &state.allowed_dirs, &state.log_files) {
                Ok(d) => d,
                Err(StatusCode::NOT_FOUND) => return Json(ApiResponse::err("directory not found")),
                Err(_) => return Json(ApiResponse::err("access denied")),
            };
            if let Err(e) = read_dir_entries(&dir, &mut entries, depth, &mut total).await {
                tracing::error!("failed to read directory {:?}: {}", dir, e);
                return Json(ApiResponse::err("Internal server error"));
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
                        return Json(ApiResponse::err("Internal server error"));
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
                    return Json(ApiResponse::err("Internal server error"));
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Json(ApiResponse::ok(FileListData { entries }))
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

async fn file_tail(
    Query(params): Query<FileTailParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<FileTailData>> {
    let path = match validate_path(&params.path, &state.allowed_dirs, &state.log_files) {
        Ok(p) => p,
        Err(StatusCode::NOT_FOUND) => return Json(ApiResponse::err("file not found")),
        Err(_) => return Json(ApiResponse::err("access denied")),
    };

    let lines = params.lines.unwrap_or(200).min(5000) as usize;

    let tail = {
        let p = path.clone();
        match tokio::task::spawn_blocking(move || LineIndex::tail_start(&p, lines)).await {
            Ok(Ok(tail)) => tail,
            _ => {
                return Json(ApiResponse::ok(FileTailData {
                    entries: Vec::new(),
                    total_lines: 0,
                }))
            }
        }
    };

    if tail.total_lines == 0 {
        return Json(ApiResponse::ok(FileTailData {
            entries: Vec::new(),
            total_lines: 0,
        }));
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

    Json(ApiResponse::ok(FileTailData {
        entries,
        total_lines: tail.total_lines,
    }))
}

async fn health(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<HealthData>> {
    Json(ApiResponse::ok(HealthData {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
    }))
}

/// Check for a newer release. Read-only — no CSRF/auth gating beyond the global
/// middleware (token still required if set, but the endpoint carries no sensitive
/// data and never mutates). Serves from cache unless `?force=true`.
async fn check_upgrade(
    Query(params): Query<UpgradeCheckParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<crate::upgrade::UpdateInfo>> {
    match state.upgrade_service.check_update(params.force.unwrap_or(false)).await {
        Ok(info) => Json(ApiResponse::ok(info)),
        Err(e) => {
            tracing::error!("failed to check update: {}", e);
            Json(ApiResponse::err("Failed to check update"))
        }
    }
}

/// Perform the upgrade: download + replace binary + delegate restart.
///
/// **Forced auth**: unlike `save_log_levels` (which only checks CSRF when token is
/// set), this endpoint *requires* a non-empty token. Replacing the running binary
/// is an RCE-class operation — it must never be reachable when auth is disabled.
/// When token is empty, the endpoint refuses with an actionable error rather than
/// silently proceeding.
async fn perform_upgrade(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<crate::upgrade::UpgradeResult>>, StatusCode> {
    // Forced auth: binary replacement is RCE-class, must require explicit token.
    if state.token.is_empty() {
        return Ok(Json(ApiResponse::err("TOKEN_REQUIRED")));
    }
    // CSRF double-check (same pattern as save_log_levels).
    if headers.get("X-Requested-With").is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    match state.upgrade_service.perform_upgrade().await {
        Ok(result) => Ok(Json(ApiResponse::ok(result))),
        Err(e) => {
            tracing::error!("upgrade failed: {}", e);
            Ok(Json(ApiResponse::err(e)))
        }
    }
}

async fn get_log_levels(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<LogLevelConfig>> {
    let config = state.level_config.load();
    Json(ApiResponse::ok(config.as_ref().clone()))
}

async fn save_log_levels(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Json(new_config): Json<LogLevelConfig>,
) -> Result<Json<ApiResponse<LogLevelConfig>>, StatusCode> {
    if !state.token.is_empty() && headers.get("X-Requested-With").is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut doc: toml::Value = if state.config_path.exists() {
        let content = std::fs::read_to_string(&state.config_path).map_err(|e| {
            tracing::error!("failed to read config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        toml::from_str(&content).map_err(|e| {
            tracing::error!("failed to parse config.toml: {}", e);
            StatusCode::BAD_REQUEST
        })?
    } else {
        toml::Value::Table(Default::default())
    };

    let config_toml = toml::to_string_pretty(&new_config).map_err(|e| {
        tracing::error!("failed to serialize log level config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let log_levels_value: toml::Value = toml::from_str(&config_toml).map_err(|e| {
        tracing::error!("failed to parse log level config as toml: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(table) = doc.as_table_mut() {
        table.insert("log_levels".to_string(), log_levels_value);
    }

    let toml_str = toml::to_string_pretty(&doc).map_err(|e| {
        tracing::error!("failed to serialize toml: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    std::fs::write(&state.config_path, toml_str).map_err(|e| {
        tracing::error!("failed to write config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let new_detector = LevelDetector::from_config(&new_config);
    state.level_detector.store(Arc::new(new_detector));
    state.level_config.store(Arc::new(new_config.clone()));

    Ok(Json(ApiResponse::ok(new_config)))
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

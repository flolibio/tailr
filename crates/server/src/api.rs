use axum::extract::{Extension, Query};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use tailr_protocol::{detect_level, try_parse_timestamp, LogEntry};
use tailr_search_engine::{LogFilter, SearchOptions};
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileListData {
    entries: Vec<FileEntry>,
}

#[derive(Deserialize)]
struct FileListParams {
    path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileContentData {
    total_lines: u64,
    offset: u64,
    limit: u64,
    has_more: bool,
    entries: Vec<LogEntry>,
}

#[derive(Deserialize)]
struct FileContentParams {
    path: String,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileTailData {
    entries: Vec<LogEntry>,
}

#[derive(Deserialize)]
struct FileTailParams {
    path: String,
    #[serde(default)]
    lines: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileInfoData {
    size: u64,
    modified: Option<String>,
    line_count: u64,
    inode: u64,
}

#[derive(Deserialize)]
struct FileInfoParams {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchData {
    matches: Vec<SearchMatchResult>,
    total_matches: usize,
    has_more: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchMatchResult {
    line_number: u64,
    content: String,
    context_before: Vec<String>,
    context_after: Vec<String>,
}

#[derive(Deserialize)]
struct SearchParams {
    path: String,
    q: String,
    #[serde(default)]
    regex: Option<bool>,
    #[serde(default)]
    levels: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthData {
    status: String,
    version: String,
    uptime_seconds: u64,
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/files", get(list_files))
        .route("/api/file/content", get(file_content))
        .route("/api/file/tail", get(file_tail))
        .route("/api/file/info", get(file_info))
        .route("/api/search", get(search))
        .route("/api/health", get(health))
}

async fn list_files(
    Query(params): Query<FileListParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<FileListData>> {
    let mut entries: Vec<FileEntry> = Vec::new();

    match params.path {
        Some(p) => {
            // List a specific directory
            let dir = PathBuf::from(p);
            if let Err(e) = read_dir_entries(&dir, &mut entries) {
                return Json(ApiResponse::err(format!("failed to read directory: {}", e)));
            }
        }
        None => {
            // Add individually specified log files
            for file in &state.log_files {
                if file.exists() && file.is_file() {
                    let metadata = std::fs::metadata(file).ok();
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
                    });
                }
            }

            // List all configured log directories
            for dir in &state.log_dirs {
                if dir.exists() && dir.is_dir() {
                    entries.push(FileEntry {
                        name: dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| dir.display().to_string()),
                        path: dir.to_string_lossy().to_string(),
                        size: 0,
                        modified: None,
                        is_dir: true,
                    });
                }
            }
            // If only one dir configured and no files, list its contents directly
            if state.log_dirs.len() == 1 && state.log_files.is_empty() {
                entries.clear();
                if let Err(e) = read_dir_entries(&state.log_dirs[0], &mut entries) {
                    return Json(ApiResponse::err(format!("failed to read directory: {}", e)));
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

fn read_dir_entries(dir: &std::path::Path, entries: &mut Vec<FileEntry>) -> std::io::Result<()> {
    let read_dir = std::fs::read_dir(dir)?;
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

        if is_dir {
            if !dir_has_text_files(&entry.path()) {
                continue;
            }
        } else if !is_text_file(&entry.path(), &name) {
            continue;
        }

        let modified = metadata
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: DateTime<Utc> = t.into();
                dt.to_rfc3339()
            });

        entries.push(FileEntry {
            name,
            path: entry.path().to_string_lossy().to_string(),
            size,
            modified,
            is_dir,
        });
    }
    Ok(())
}

fn dir_has_text_files(dir: &std::path::Path) -> bool {
    dir_has_text_files_inner(dir, 0)
}

fn dir_has_text_files_inner(dir: &std::path::Path, depth: u32) -> bool {
    if depth > 2 {
        return true;
    }
    let read_dir = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            if dir_has_text_files_inner(&entry.path(), depth + 1) {
                return true;
            }
        } else if is_text_file(&entry.path(), &name) {
            return true;
        }
    }
    false
}

fn is_text_file(path: &std::path::Path, _name: &str) -> bool {
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
        return is_likely_text(path);
    }

    is_likely_text(path)
}

fn is_likely_text(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = [0u8; 512];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n == 0 {
        return true;
    }
    !buf[..n].contains(&0)
}

async fn file_content(
    Query(params): Query<FileContentParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ApiResponse<FileContentData>>, StatusCode> {
    let path = PathBuf::from(&params.path);
    if !path.exists() {
        return Ok(Json(ApiResponse::err("file not found")));
    }

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(1000);

    let index = get_or_build_index(&state, &path).await;
    let total_lines = index.total_lines();

    if offset >= total_lines {
        return Ok(Json(ApiResponse::ok(FileContentData {
            total_lines,
            offset,
            limit,
            has_more: false,
            entries: Vec::new(),
        })));
    }

    let start_byte = match index.offset_of_line(offset) {
        Some(b) => b,
        None => return Ok(Json(ApiResponse::err("invalid offset"))),
    };

    let entries = read_lines_from(&path, start_byte, limit as usize, offset).await;
    let end_offset = offset + entries.len() as u64;
    let has_more = end_offset < total_lines;

    Ok(Json(ApiResponse::ok(FileContentData {
        total_lines,
        offset,
        limit,
        has_more,
        entries,
    })))
}

async fn file_tail(
    Query(params): Query<FileTailParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<FileTailData>> {
    let path = PathBuf::from(&params.path);
    if !path.exists() {
        return Json(ApiResponse::err("file not found"));
    }

    let lines = params.lines.unwrap_or(100).min(5000) as usize;

    let index = get_or_build_index(&state, &path).await;
    let total = index.total_lines();

    if total == 0 {
        return Json(ApiResponse::ok(FileTailData {
            entries: Vec::new(),
        }));
    }

    let start_line = total.saturating_sub(lines as u64);
    let start_byte = index.offset_of_line(start_line).unwrap_or(0);

    let entries = read_lines_from(&path, start_byte, lines, start_line).await;

    Json(ApiResponse::ok(FileTailData { entries }))
}

async fn file_info(
    Query(params): Query<FileInfoParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<FileInfoData>> {
    let path = PathBuf::from(&params.path);
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) => return Json(ApiResponse::err(format!("failed to stat file: {}", e))),
    };

    let modified = metadata
        .modified()
        .ok()
        .map(|t| {
            let dt: DateTime<Utc> = t.into();
            dt.to_rfc3339()
        });

    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        metadata.ino()
    };
    #[cfg(not(unix))]
    let inode = 0;

    let index = get_or_build_index(&state, &path).await;
    let line_count = index.total_lines();

    Json(ApiResponse::ok(FileInfoData {
        size: metadata.len(),
        modified,
        line_count,
        inode,
    }))
}

async fn search(
    Query(params): Query<SearchParams>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ApiResponse<SearchData>> {
    let path = PathBuf::from(&params.path);
    if !path.exists() {
        return Json(ApiResponse::err("file not found"));
    }

    let context = params.context.unwrap_or(3);
    let limit = params.limit.unwrap_or(100);
    let is_regex = params.regex.unwrap_or(false);

    let all_levels: Vec<tailr_protocol::LogLevel> = params
        .levels
        .map(|s| {
            s.split(',')
                .filter_map(|l| match l.trim().to_uppercase().as_str() {
                    "ALERT" => Some(tailr_protocol::LogLevel::ALERT),
                    "ERROR" => Some(tailr_protocol::LogLevel::ERROR),
                    "WARN" => Some(tailr_protocol::LogLevel::WARN),
                    "INFO" => Some(tailr_protocol::LogLevel::INFO),
                    "DEBUG" => Some(tailr_protocol::LogLevel::DEBUG),
                    "TRACE" => Some(tailr_protocol::LogLevel::TRACE),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let time_from = params.from.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });
    let time_to = params.to.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    let opts = SearchOptions {
        pattern: params.q.clone(),
        is_regex,
        case_insensitive: false,
        context_before: context,
        context_after: context,
        max_results: limit,
        level_filter: all_levels.first().cloned(),
    };

    let result = match state.search_engine.search(&path, &opts) {
        Ok(r) => r,
        Err(e) => return Json(ApiResponse::err(format!("search failed: {}", e))),
    };

    let filter = LogFilter::new()
        .with_levels(all_levels)
        .with_time(time_from, time_to);

    let matches: Vec<SearchMatchResult> = result
        .matches
        .into_iter()
        .filter(|m| {
            if !filter.levels.is_empty() || filter.time_from.is_some() || filter.time_to.is_some() {
                let entry = tailr_protocol::LogEntry {
                    line_num: m.line_num,
                    raw: m.content.clone(),
                    level: detect_level(&m.content),
                    timestamp: None,
                    fields: None,
                };
                filter.matches(&entry)
            } else {
                true
            }
        })
        .map(|m| SearchMatchResult {
            line_number: m.line_num,
            content: m.content,
            context_before: m.context_before,
            context_after: m.context_after,
        })
        .collect();

    let total_matches = result.total_matches;
    let has_more = result.has_more;

    Json(ApiResponse::ok(SearchData {
        matches,
        total_matches,
        has_more,
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

async fn get_or_build_index(state: &AppState, path: &PathBuf) -> LineIndex {
    if let Some(entry) = state.line_indices.get(path) {
        let idx = entry.value().clone();
        let file_size = tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
        if file_size > idx.file_size {
            drop(entry);
            let mut idx_mut = idx.clone();
            if idx_mut.update(path, file_size).is_ok() {
                state.line_indices.insert(path.clone(), idx_mut.clone());
                return idx_mut;
            }
        }
        return idx;
    }
    match LineIndex::build(path) {
        Ok(idx) => {
            state.line_indices.insert(path.clone(), idx.clone());
            idx
        }
        Err(_) => LineIndex::new(),
    }
}

async fn read_lines_from(path: &PathBuf, start_byte: u64, max_lines: usize, base_line: u64) -> Vec<LogEntry> {
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

        let level = detect_level(trimmed);
        let timestamp = try_parse_timestamp(trimmed);

        entries.push(LogEntry {
            line_num,
            raw: trimmed.to_string(),
            level,
            timestamp,
            fields: None,
        });

        line_num += 1;
        if entries.len() >= max_lines {
            break;
        }
    }

    entries
}

//! MCP (Model Context Protocol) 服务端 — `/mcp` streamable HTTP 端点。
//!
//! 让 AI agent（Claude Code / Cursor 等）直接检索服务器日志：搜索、读尾部、
//! 读任意游标、级别统计。设计决策见知识库《tailr-MCP-AI日志访问层设计》：
//!
//! - **token 防护**：所有 tool 的输出都经 QueryService 预算钳制（命中数/
//!   字节/超时），响应自带 `resumeCursor` 分页语义；tool description 与
//!   参数 doc 注释就是给 LLM 的提示词（内嵌工作流引导：统计先行、节制
//!   行数、翻页续扫），改动前先看设计文档补充项第 7 条。
//! - **无索引**：cursor（字节偏移+行号）原语，多 GB 文件零预建索引。
//! - **取消**：客户端断开时 Drop guard 置位 AtomicBool，扫描在下一块
//!   边界停止（axum 会 drop 请求 future，但 spawn_blocking 线程不会自己停）。
//! - **挂载**：`/mcp` 走 auth_middleware（Bearer token，与 REST/WS 同源），
//!   不挂 governor——AI 一次排查密集调用是正常模式，重活由 QueryService
//!   的并发闸（semaphore）保护，HTTP 层限流会造成合法 agent 被 429 风暴。
//!
//! 注意：rmcp 锁定 0.5.x。main 分支已支持裸参数 tool 方法与 Host/Origin
//! 校验配置，0.5.0 均没有——参数必须走 `Parameters<结构体>`，升级时同步调整。

use std::future::Future; // tool 宏展开体引用
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Parameters;
use rmcp::model::*;
use rmcp::tool_handler;
use rmcp::{
    ErrorData as McpError, ServerHandler, schemars, tool, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService,
        session::local::LocalSessionManager,
    },
};
use serde_json::json;
use tailr_core::query::{QueryError, SearchRequest};
use tailr_search_engine::ScanResult;
use tailr_tail_engine::LineIndex;

use crate::AppState;

/// 单行输出截断（与 scanner 的截断语义一致，tail 路径手动应用）。
const MAX_LINE_BYTES: usize = 4 * 1024;
const LINE_TRUNCATION_MARKER: &str = "…[line truncated]";
const MAX_TAIL_LINES: usize = 5000;
/// MCP 列文件的递归深度：浏览器约定 ≤2，AI 不需要更深的树。
const MCP_LIST_DEPTH: u32 = 2;

/// 客户端断开取消：请求 future 被 drop（axum/hyper 断连行为）时置位标志，
/// spawn_blocking 里的扫描循环在下一个 4MB 块边界看到并停止。
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn truncate_line(text: &str) -> String {
    if text.len() <= MAX_LINE_BYTES {
        return text.to_string();
    }
    let mut cut = MAX_LINE_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{}", &text[..cut], LINE_TRUNCATION_MARKER)
}

/// 游标编解码：`"{字节偏移}:{行号}"`。不透明给客户端，服务端可自行演进格式。
fn encode_cursor(offset: u64, line: u64) -> String {
    format!("{offset}:{line}")
}

fn decode_cursor(s: &str) -> Result<(u64, u64), McpError> {
    let bad = |msg: String| McpError::invalid_params(msg, None);
    let (off, line) = s
        .split_once(':')
        .ok_or_else(|| bad(format!("malformed cursor: {s:?} (expected \"offset:line\")")))?;
    let off: u64 = off
        .parse()
        .map_err(|_| bad(format!("cursor offset not a number: {off:?}")))?;
    let line: u64 = line
        .parse()
        .map_err(|_| bad(format!("cursor line not a number: {line:?}")))?;
    Ok((off, line.max(1)))
}

/// tailr ErrorCode（validate_path 的返回）→ MCP 错误。
fn path_error_to_mcp(code: tailr_core::error::ErrorCode) -> McpError {
    match code {
        tailr_core::error::ErrorCode::NotFound => {
            McpError::resource_not_found("log file not found", None)
        }
        tailr_core::error::ErrorCode::PathNotAllowed => McpError::invalid_params(
            "path is outside the configured log directories",
            None,
        ),
        other => McpError::internal_error(format!("path validation failed: {other}"), None),
    }
}

fn query_error_to_mcp(e: QueryError) -> McpError {
    match e {
        QueryError::Busy => McpError::new(
            ErrorCode(-32000), // 自定义码：并发扫描已满（非客户端参数错误）
            "scan concurrency limit reached — wait a moment and retry the same request",
            None,
        ),
        QueryError::TooManyKeywords(max) => {
            McpError::invalid_params(format!("too many keywords (max {max})"), None)
        }
        QueryError::KeywordTooLong(max) => {
            McpError::invalid_params(format!("keyword too long (max {max} bytes)"), None)
        }
        QueryError::Io(e) => McpError::internal_error(format!("log file I/O error: {e}"), None),
    }
}

fn query_io(e: std::io::Error) -> McpError {
    query_error_to_mcp(QueryError::Io(e))
}

fn scan_result_json(state: &AppState, path: &str, r: &ScanResult) -> serde_json::Value {
    json!({
        "host": state.host_name,
        "path": path,
        "windows": r.windows.iter().map(|w| json!({
            "startLine": w.start_line,
            "endLine": w.end_line,
            "lines": w.entries.iter().map(|e| json!({
                "line": e.line_no,
                "text": e.text,
                "isMatch": e.is_match,
                "truncated": e.line_truncated,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "matchedLines": r.matched_lines,
        "linesScanned": r.lines_scanned,
        "more": r.has_more(),
        "truncated": r.truncated,
        "timedOut": r.timed_out,
        "resumeCursor": encode_cursor(r.resume_offset, r.resume_line),
    })
}

// ---- Tool 参数结构体（schemars 的 doc 注释 = 参数描述，给 LLM 看的提示词）----

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TailLogArgs {
    /// Absolute path of the log file, exactly as returned by list_log_files
    pub path: String,
    /// How many lines to read from the end (default 100, max 5000)
    pub lines: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchLogsArgs {
    /// Absolute path of the log file, exactly as returned by list_log_files
    pub path: String,
    /// Keywords that must ALL appear in a line (AND, case-insensitive). Use 2-3 specific ones to keep matches low.
    pub keywords: Vec<String>,
    /// true = only COUNT matching lines, no line content returned. Much faster per page and near-zero output tokens — use this for any "how many" question, especially when matches are dense.
    pub count_only: Option<bool>,
    /// Context lines before/after each match (default 2, max 10)
    pub context_lines: Option<usize>,
    /// Max matched lines to return per page (default 50, server caps it)
    pub max_matches: Option<usize>,
    /// Scan time budget in milliseconds (default 10000, server caps it)
    pub timeout_ms: Option<u64>,
    /// Pass the resumeCursor from the previous page to continue; never restart from the beginning
    pub resume_cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadLogRangeArgs {
    /// Absolute path of the log file, exactly as returned by list_log_files
    pub path: String,
    /// Pass the resumeCursor from a previous page to continue; omit to start from the beginning of the file
    pub resume_cursor: Option<String>,
    /// Max lines to return (default 100)
    pub max_lines: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetLogStatsArgs {
    /// Absolute path of the log file, exactly as returned by list_log_files
    pub path: String,
}

pub struct McpTools {
    state: Arc<AppState>,
    tool_router: ToolRouter<Self>,
}

impl McpTools {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// 拿到通过路径校验的绝对路径（与 REST /api 同一套 allowlist）。
    fn validated_path(&self, path: &str) -> Result<std::path::PathBuf, McpError> {
        crate::api::validate_path(path, &self.state.allowed_dirs, &self.state.log_files)
            .map_err(path_error_to_mcp)
    }

    /// search/read 共用管道：路径校验 → 游标解析 → Drop guard 取消 →
    /// spawn_blocking 里调 core QueryService。
    async fn run_search(
        &self,
        path: &str,
        request: SearchRequest,
        resume_cursor: &Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let path_buf = self.validated_path(path)?;
        let (start_offset, start_line) = match resume_cursor {
            Some(c) => decode_cursor(c)?,
            None => (0, 1),
        };
        let mut request = request;
        request.start_offset = start_offset;
        request.start_line = start_line;

        let state = self.state.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let _guard = CancelOnDrop(cancel.clone());
        let result = tokio::task::spawn_blocking(move || {
            state
                .query
                .search(&path_buf, &request, Some(&cancel))
                .map(|r| (state.clone(), r))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("search task failed: {e}"), None))?
        .map_err(query_error_to_mcp)?;

        Ok(CallToolResult::success(vec![Content::text(
            scan_result_json(&result.0, path, &result.1).to_string(),
        )]))
    }
}

#[tool_router]
impl McpTools {
    #[tool(description = "List the log files accessible on this server. Returns name/path/size/isDir for each entry. Start here to discover what you can query — the `path` values are the exact identifiers the other tools expect.")]
    async fn list_log_files(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.clone();
        let entries = tokio::task::spawn_blocking(move || list_files_blocking(&state))
            .await
            .map_err(|e| McpError::internal_error(format!("listing task failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(
            json!({
                "host": self.state.host_name,
                "files": entries,
            })
            .to_string(),
        )]))
    }

    #[tool(description = "Read the LAST N lines of a log file (default 100, max 5000) with absolute line numbers. Good for a quick look at recent activity; prefer get_log_stats + search_logs for anything bigger.")]
    async fn tail_log(
        &self,
        Parameters(args): Parameters<TailLogArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path_buf = self.validated_path(&args.path)?;
        let n = args.lines.unwrap_or(100).clamp(1, MAX_TAIL_LINES);

        let state = self.state.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let _guard = CancelOnDrop(cancel.clone());
        let out = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, McpError> {
            // start_byte 反向扫描即精确；total_lines 在长文件下是平均行长
            // 估算——行号必须精确（要和 search_logs 的行号一致），所以用
            // scanner 数一遍（经 QueryService 并发闸）。
            let tail = LineIndex::tail_start(&path_buf, n).map_err(query_io)?;
            let stats = state
                .query
                .stats(&path_buf, None, Some(&cancel))
                .map_err(query_error_to_mcp)?;
            let total_lines = stats.total_lines;
            if total_lines == 0 {
                return Ok(json!({
                    "lines": [],
                    "totalLines": 0,
                    "sizeBytes": stats.total_bytes,
                }));
            }
            let start_line = total_lines.saturating_sub(n as u64) + 1;
            // 只读尾部窗口（seek + read），多 GB 文件也只占 N 行内存。
            let mut file = std::fs::File::open(&path_buf).map_err(query_io)?;
            use std::io::{Read, Seek, SeekFrom};
            let mut buf = Vec::new();
            file.seek(SeekFrom::Start(tail.start_byte)).map_err(query_io)?;
            file.take(64 * 1024 * 1024).read_to_end(&mut buf).map_err(query_io)?;
            let mut line_no = start_line;
            let lines: Vec<serde_json::Value> = buf
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .map(|l| {
                    let mut end = l.len();
                    if end > 0 && l[end - 1] == b'\r' {
                        end -= 1;
                    }
                    let text = truncate_line(&String::from_utf8_lossy(&l[..end]));
                    let no = line_no;
                    line_no += 1;
                    json!({ "line": no, "text": text })
                })
                .collect();
            Ok(json!({
                "lines": lines,
                "totalLines": total_lines,
                "sizeBytes": stats.total_bytes,
            }))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("tail task failed: {e}"), None))??;

        let mut full = json!({ "host": self.state.host_name, "path": args.path });
        if let (Some(dst), Some(src)) = (full.as_object_mut(), out.as_object()) {
            for (k, v) in src {
                dst.insert(k.clone(), v.clone());
            }
        }
        Ok(CallToolResult::success(vec![Content::text(
            full.to_string(),
        )]))
    }

    #[tool(description = "Search a log file for lines containing ALL keywords (AND, case-insensitive). For 'how many/how often' questions set count_only=true (fast, counts without content). For inspection, returns matching lines merged into context windows (± context_lines). Workflow: get_log_stats first, then search with 2-3 specific keywords. If more/truncated is true, continue with resumeCursor (never restart); a timeout also returns partial results + resumeCursor — just continue. When reporting results, state the raw matched LINE count first, then any higher-level grouping you derive (e.g. request-pairs) — users usually think in lines.")]
    async fn search_logs(
        &self,
        Parameters(args): Parameters<SearchLogsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let request = SearchRequest {
            keywords: args.keywords,
            count_only: args.count_only.unwrap_or(false),
            max_matches: args.max_matches,
            time_budget_ms: args.timeout_ms,
            context_before: args.context_lines,
            context_after: args.context_lines,
            ..SearchRequest::default()
        };
        self.run_search(&args.path, request, &args.resume_cursor).await
    }

    #[tool(description = "Read a log file sequentially line by line. Without resumeCursor starts from the beginning; otherwise continues exactly where a previous search_logs/read_log_range page stopped. Returns up to max_lines (default 100) lines plus the next resumeCursor. Reading whole huge logs wastes tokens — use search_logs unless you truly need sequential reading (small files or a known region).")]
    async fn read_log_range(
        &self,
        Parameters(args): Parameters<ReadLogRangeArgs>,
    ) -> Result<CallToolResult, McpError> {
        // 顺序读 = 空关键字搜索：无窗口上下文（context 0），同一套预算/
        // 取消/游标机制，无额外代码路径。
        let request = SearchRequest {
            keywords: Vec::new(),
            max_matches: args.max_lines,
            context_before: Some(0),
            context_after: Some(0),
            ..SearchRequest::default()
        };
        self.run_search(&args.path, request, &args.resume_cursor).await
    }

    #[tool(description = "Get statistics for one log file: total lines, size, per-level line counts (ERROR/WARN/INFO..., using this server's configured level keywords). Cheap way to understand a file before searching — '3 ERROR lines' tells you how narrow to make your keywords. Multi-GB files need a full scan; if incomplete is true the counts are partial.")]
    async fn get_log_stats(
        &self,
        Parameters(args): Parameters<GetLogStatsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path_buf = self.validated_path(&args.path)?;
        let detector = self.state.level_detector.load();

        let state = self.state.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let _guard = CancelOnDrop(cancel.clone());
        let stats = tokio::task::spawn_blocking(move || {
            state
                .query
                .stats(&path_buf, Some(&detector), Some(&cancel))
                .map(|s| (state.host_name.clone(), s))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("stats task failed: {e}"), None))?
        .map_err(query_error_to_mcp)?;

        let (host, s) = stats;
        Ok(CallToolResult::success(vec![Content::text(
            json!({
                "host": host,
                "path": args.path,
                "totalLines": s.total_lines,
                "sizeBytes": s.total_bytes,
                "sizeMB": (s.total_bytes as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0,
                "levels": s.levels.iter().map(|(n, c)| json!({"level": n, "lines": c})).collect::<Vec<_>>(),
                "unknownLines": s.unknown_lines,
                "incomplete": !s.completed,
            })
            .to_string(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for McpTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "tailr".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Search-oriented log access. Workflow: list_log_files → \
                 get_log_stats → search_logs (AND keywords, case-insensitive) → \
                 tail/read for context. Every response includes `host` so you can \
                 tell multi-server results apart. Paginate with resumeCursor; \
                 never re-request from the start."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

/// 同步列文件（spawn_blocking 里跑）：配置的 log_files 平铺 + log_dirs
/// 递归（深度 ≤ MCP_LIST_DEPTH），只留文本文件，平铺输出（AI 不要树）。
fn list_files_blocking(state: &AppState) -> Vec<serde_json::Value> {
    let mut out = Vec::new();

    for file in &state.log_files {
        if let Ok(meta) = std::fs::metadata(file) {
            if meta.is_file() {
                out.push(json!({
                    "name": file.file_name().map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.display().to_string()),
                    "path": file.display().to_string(),
                    "sizeBytes": meta.len(),
                    "isDir": false,
                }));
            }
        }
    }

    let mut total = 0;
    for dir in &state.log_dirs {
        collect_text_files(dir, MCP_LIST_DEPTH, &mut out, &mut total);
    }

    out
}

fn collect_text_files(
    dir: &std::path::Path,
    depth: u32,
    out: &mut Vec<serde_json::Value>,
    total: &mut usize,
) {
    if depth == 0 || *total >= crate::api::MAX_LIST_ENTRIES {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        if *total >= crate::api::MAX_LIST_ENTRIES {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let path = entry.path();
        if meta.is_dir() {
            collect_text_files(&path, depth - 1, out, total);
        } else {
            if !crate::api::is_text_file_blocking(&path) {
                continue;
            }
            *total += 1;
            out.push(json!({
                "name": name,
                "path": path.display().to_string(),
                "sizeBytes": meta.len(),
                "isDir": false,
            }));
        }
    }
}

/// 构造 `/mcp` 的 router（auth 中间件内，governor 外——见模块头注释）。
pub fn routes(state: Arc<AppState>) -> axum::Router {
    // 无服务端会话状态（stateful_mode=false）：每个 POST 独立处理，客户端
    // 断线重连零成本。0.5.0 尚无 Host/Origin 校验配置（上游 main 已加），
    // 接入安全由 auth_middleware 的 Bearer token 承担（与 REST/WS 同一策略）；
    // 升级 rmcp 后补 [mcp] allowed_hosts/allowed_origins 配置。
    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(std::time::Duration::from_secs(15)),
        stateful_mode: false,
    };

    let service: StreamableHttpService<McpTools, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(McpTools::new(state.clone())),
            Default::default(),
            config,
        );

    axum::Router::new().route_service("/mcp", service)
}

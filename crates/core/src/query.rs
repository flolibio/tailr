//! 日志查询服务（MCP search/stats 的领域入口）
//!
//! 职责（见知识库 tailr-MCP 设计 2026-08-14「扫描的超时与取消设计」）：
//! - **预算钳制**：客户端可调小、不可超过服务端硬上限（token 防护的服务端一侧）
//! - **并发闸**：try_acquire 信号量，拿不到立即拒绝（不排队——排队会让
//!   agent 侧的超时语义失真）。扫描本身是同步阻塞计算，调用方（Web/MCP
//!   层）负责包在 `spawn_blocking` 里执行。
//!
//! 本模块是纯同步 `fn`：无 HTTP、无 async、无运行时假设，符合 core 边界。

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tailr_search_engine::{
    file_stats, scan_file, FileStats, LevelDetector, ScanParams, ScanResult, DEFAULT_CONTEXT,
    DEFAULT_MAX_BYTES, DEFAULT_MAX_MATCHES, DEFAULT_TIME_BUDGET,
};

/// 服务端硬上限集合。默认值是出厂策略，将来到自 config（additive 扩展）。
#[derive(Debug, Clone)]
pub struct QueryCaps {
    /// 并发扫描闸（semaphore 许可数）。
    pub max_concurrent_scans: usize,
    /// 单次返回的命中数上限。
    pub max_matches_hard: usize,
    /// 单次响应的输出字节上限。
    pub max_bytes_hard: usize,
    /// 单次扫描的最长时间预算。
    pub max_time_budget: Duration,
    /// before/after 上下文行数上限。
    pub max_context: usize,
    /// 关键字数量上限（AND 语义）。
    pub max_keywords: usize,
    /// 单个关键字长度上限。
    pub max_keyword_len: usize,
    /// 单行输出截断长度。
    pub max_line_bytes: usize,
}

impl Default for QueryCaps {
    fn default() -> Self {
        Self {
            max_concurrent_scans: 4,
            max_matches_hard: 200,
            max_bytes_hard: 512 * 1024,
            max_time_budget: Duration::from_secs(30),
            max_context: 10,
            max_keywords: 8,
            max_keyword_len: 256,
            max_line_bytes: 4 * 1024,
        }
    }
}

/// 一次搜索请求。`None` 字段使用 scanner 默认值，再被 `QueryCaps` 钳制。
#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    pub keywords: Vec<String>,
    pub start_offset: u64,
    pub start_line: u64,
    pub max_matches: Option<usize>,
    pub max_bytes: Option<usize>,
    pub time_budget_ms: Option<u64>,
    pub context_before: Option<usize>,
    pub context_after: Option<usize>,
    /// 只计数不输出行内容（计数型查询；密集匹配时唯一可行的方式）。
    pub count_only: bool,
}

/// 查询失败。core 只陈述事实（基线英文描述），表现层决定如何呈现
/// （Web → HTTP 状态，MCP → JSON-RPC error）。
#[derive(Debug)]
pub enum QueryError {
    /// 并发扫描已满。语义上同限流：立即返回，让客户端自行决定重试。
    Busy,
    /// 关键字数量超限（携带实际上限）。
    TooManyKeywords(usize),
    /// 关键字过长（携带允许的最大长度）。
    KeywordTooLong(usize),
    /// 文件 I/O 错误（文件消失、权限等）。
    Io(std::io::Error),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Busy => write!(f, "scan concurrency limit reached, retry later"),
            QueryError::TooManyKeywords(max) => {
                write!(f, "too many keywords (max {max})")
            }
            QueryError::KeywordTooLong(max) => {
                write!(f, "keyword too long (max {max} bytes)")
            }
            QueryError::Io(e) => write!(f, "log file I/O error: {e}"),
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            QueryError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// 已完成统计的缓存条目：追加型日志的 (mtime, size) 命中率极高，
/// 一次全扫的精确行数可服务后续所有 stats/tail 调用。
struct CachedStats {
    mtime: SystemTime,
    size: u64,
    stats: FileStats,
}

pub struct QueryService {
    semaphore: tokio::sync::Semaphore,
    caps: QueryCaps,
    stats_cache: Mutex<HashMap<PathBuf, CachedStats>>,
}

impl QueryService {
    pub fn new(caps: QueryCaps) -> Self {
        let permits = caps.max_concurrent_scans.max(1);
        Self {
            semaphore: tokio::sync::Semaphore::new(permits),
            caps,
            stats_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn caps(&self) -> &QueryCaps {
        &self.caps
    }

    /// 关键字搜索。同步阻塞（可能扫满整个时间预算），调用方需在
    /// `spawn_blocking` 中执行；`cancel` 在客户端断开时置位。
    pub fn search(
        &self,
        path: &Path,
        request: &SearchRequest,
        cancel: Option<&AtomicBool>,
    ) -> Result<ScanResult, QueryError> {
        if request.keywords.len() > self.caps.max_keywords {
            return Err(QueryError::TooManyKeywords(self.caps.max_keywords));
        }
        for k in &request.keywords {
            if k.len() > self.caps.max_keyword_len {
                return Err(QueryError::KeywordTooLong(self.caps.max_keyword_len));
            }
        }

        let params = ScanParams {
            keywords: request.keywords.clone(),
            start_offset: request.start_offset,
            // 行号从 1 开始；0 视为无效输入归一化为 1。
            start_line: request.start_line.max(1),
            max_matches: request
                .max_matches
                .unwrap_or(DEFAULT_MAX_MATCHES)
                .min(self.caps.max_matches_hard),
            max_bytes: request
                .max_bytes
                .unwrap_or(DEFAULT_MAX_BYTES)
                .min(self.caps.max_bytes_hard),
            time_budget: request
                .time_budget_ms
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_TIME_BUDGET)
                .min(self.caps.max_time_budget),
            context_before: request
                .context_before
                .unwrap_or(DEFAULT_CONTEXT)
                .min(self.caps.max_context),
            context_after: request
                .context_after
                .unwrap_or(DEFAULT_CONTEXT)
                .min(self.caps.max_context),
            max_line_bytes: self.caps.max_line_bytes,
            count_only: request.count_only,
        };

        let _permit = self
            .semaphore
            .try_acquire()
            .map_err(|_| QueryError::Busy)?;
        scan_file(path, &params, cancel).map_err(QueryError::Io)
    }

    /// 文件统计。同样占并发闸（全文件扫描同样是重活）。
    ///
    /// 缓存策略：以 (mtime, size) 为键缓存**已完成**的统计——追加型日志
    /// 的连续 tail/stats 调用不再每次全扫；文件增长自动失效。未完成的
    /// 部分计数不缓存。缓存命中不占并发闸（无扫描发生）。
    pub fn stats(
        &self,
        path: &Path,
        detector: Option<&LevelDetector>,
        cancel: Option<&AtomicBool>,
    ) -> Result<FileStats, QueryError> {
        let meta = std::fs::metadata(path).map_err(QueryError::Io)?;
        let (meta_mtime, meta_size) = (meta.modified().map_err(QueryError::Io)?, meta.len());

        if let Ok(cache) = self.stats_cache.lock() {
            if let Some(hit) = cache.get(path) {
                if hit.mtime == meta_mtime && hit.size == meta_size {
                    return Ok(hit.stats.clone());
                }
            }
        }

        let _permit = self
            .semaphore
            .try_acquire()
            .map_err(|_| QueryError::Busy)?;
        let stats =
            file_stats(path, detector, self.caps.max_time_budget, cancel).map_err(QueryError::Io)?;
        if stats.completed {
            if let Ok(mut cache) = self.stats_cache.lock() {
                cache.insert(
                    path.to_path_buf(),
                    CachedStats {
                        mtime: meta_mtime,
                        size: meta_size,
                        stats: stats.clone(),
                    },
                );
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_log(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn request(keywords: &[&str]) -> SearchRequest {
        SearchRequest {
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            ..SearchRequest::default()
        }
    }

    #[test]
    fn hard_caps_clamp_large_client_budgets() {
        let f = write_log(&"hit\n".repeat(1000));
        let svc = QueryService::new(QueryCaps {
            max_matches_hard: 5,
            ..QueryCaps::default()
        });
        let mut req = request(&["hit"]);
        req.max_matches = Some(1000);
        let r = svc.search(f.path(), &req, None).unwrap();
        assert!(r.truncated);
        assert_eq!(r.matched_lines, 5);
    }

    #[test]
    fn busy_when_permits_exhausted() {
        let f = write_log("one hit\n");
        let svc = QueryService::new(QueryCaps {
            max_concurrent_scans: 1,
            ..QueryCaps::default()
        });
        // 手动占满唯一的许可（mod tests 可访问私有字段，确定性验证）
        let _held = svc.semaphore.try_acquire().unwrap();
        let err = svc.search(f.path(), &request(&["hit"]), None).unwrap_err();
        assert!(matches!(err, QueryError::Busy));
        let err2 = svc.stats(f.path(), None, None).unwrap_err();
        assert!(matches!(err2, QueryError::Busy));
    }

    #[test]
    fn keyword_validation() {
        let f = write_log("x\n");
        let svc = QueryService::new(QueryCaps::default());
        let many: Vec<String> = (0..9).map(|i| i.to_string()).collect();
        let err = svc
            .search(
                f.path(),
                &SearchRequest {
                    keywords: many,
                    ..SearchRequest::default()
                },
                None,
            )
            .unwrap_err();
        assert!(matches!(err, QueryError::TooManyKeywords(8)));

        let err = svc
            .search(
                f.path(),
                &SearchRequest {
                    keywords: vec!["k".repeat(257)],
                    ..SearchRequest::default()
                },
                None,
            )
            .unwrap_err();
        assert!(matches!(err, QueryError::KeywordTooLong(256)));
    }

    #[test]
    fn stats_cache_invalidates_on_file_change() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(f, "one").unwrap();
        f.flush().unwrap();
        let svc = QueryService::new(QueryCaps::default());

        let s1 = svc.stats(f.path(), None, None).unwrap();
        assert_eq!(s1.total_lines, 1);

        // 追加后 (mtime,size) 变化 → 缓存失效 → 重新扫描
        writeln!(f, "two").unwrap();
        f.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let s2 = svc.stats(f.path(), None, None).unwrap();
        assert_eq!(s2.total_lines, 2);

        // 未变化 → 命中缓存返回同一结果
        let s3 = svc.stats(f.path(), None, None).unwrap();
        assert_eq!(s3.total_lines, 2);
    }

    #[test]
    fn search_and_stats_passthrough() {
        let f = write_log("INFO a\nERROR b\n");
        let svc = QueryService::new(QueryCaps::default());
        let r = svc.search(f.path(), &request(&["error"]), None).unwrap();
        assert_eq!(r.matched_lines, 1);
        let s = svc.stats(f.path(), None, None).unwrap();
        assert_eq!(s.total_lines, 2);
        assert!(s.completed);
    }
}

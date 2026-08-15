//! Cursor 式日志扫描器（MCP search_logs / read 场景的地基）
//!
//! 设计要点（见知识库 tailr-MCP-AI日志访问层设计 2026-08-14 决策）：
//! - 原语是字节偏移游标，不是行号索引——无预建索引，多 GB 文件零额外内存
//! - 纯同步计算（`fn`），调用方（Web/MCP 层）自行包 `spawn_blocking`
//! - 分块扫描（4MB），每块边界检查时间预算与取消标志；预算耗尽返回
//!   已有结果 + resume 游标，把"超时"变成分页而非错误
//! - token 防护：结果数 / 输出字节双硬上限，单行长度截断

use memchr::{memchr, memrchr};
use memmap2::Mmap;
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 预算检查的字块粒度。扫描循环只在块边界看时钟/取消标志，
/// 避免每行一次 `Instant::now()` 的开销。
const CHUNK_SIZE: usize = 4 * 1024 * 1024;

pub const DEFAULT_MAX_MATCHES: usize = 50;
pub const DEFAULT_MAX_BYTES: usize = 128 * 1024;
pub const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(10);
pub const DEFAULT_CONTEXT: usize = 2;
/// 单行输出截断长度。日志里的巨型 JSON 行是 token 杀手，超长行截断加标记。
pub const DEFAULT_MAX_LINE_BYTES: usize = 2 * 1024;
const LINE_TRUNCATION_MARKER: &str = "…[line truncated]";

/// 一次扫描的全部参数。预算字段是"客户端可调小、服务端封顶"的默认值。
#[derive(Debug, Clone)]
pub struct ScanParams {
    /// AND 语义的关键字列表（ASCII 大小写不敏感）。空列表 = 不过滤（全行命中）。
    pub keywords: Vec<String>,
    /// 起始字节偏移（必须在行首；由本扫描器产出的 resume 游标保证）。
    pub start_offset: u64,
    /// 起始偏移对应行号（游标的一部分，避免 resume 时重新数行）。
    pub start_line: u64,
    pub max_matches: usize,
    pub max_bytes: usize,
    pub time_budget: Duration,
    pub context_before: usize,
    pub context_after: usize,
    pub max_line_bytes: usize,
    /// 只计数不输出：跳过上下文窗口/环形缓冲/文本物化，纯匹配循环。
    /// 「这个关键字有多少行」类问题在密集匹配下靠翻页数完不可行，
    /// count 模式让单页吞吐更高、返回体只有几百字节。
    pub count_only: bool,
}

impl Default for ScanParams {
    fn default() -> Self {
        Self {
            keywords: Vec::new(),
            start_offset: 0,
            start_line: 1,
            max_matches: DEFAULT_MAX_MATCHES,
            max_bytes: DEFAULT_MAX_BYTES,
            time_budget: DEFAULT_TIME_BUDGET,
            context_before: DEFAULT_CONTEXT,
            context_after: DEFAULT_CONTEXT,
            max_line_bytes: DEFAULT_MAX_LINE_BYTES,
            count_only: false,
        }
    }
}

/// 输出中的一行。`is_match` 区分命中行与上下文行。
#[derive(Debug, Clone)]
pub struct LineEntry {
    pub line_no: u64,
    pub offset: u64,
    pub text: String,
    /// 原始行超出 `max_line_bytes` 被截断。
    pub line_truncated: bool,
    pub is_match: bool,
}

/// 一个上下文窗口：一次命中及其前后文；相邻命中（间隔 ≤ context_after）合并进同一窗口。
#[derive(Debug, Clone)]
pub struct ContextWindow {
    pub start_line: u64,
    pub end_line: u64,
    pub entries: Vec<LineEntry>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub windows: Vec<ContextWindow>,
    /// 已收集的命中行数（截断后即已收集数，总数需翻页获知）。
    pub matched_lines: u64,
    pub lines_scanned: u64,
    /// 续扫游标：始终指向"下一条未处理行"的行首。
    pub resume_offset: u64,
    pub resume_line: u64,
    pub eof_reached: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    /// 预算（max_matches / max_bytes）触发截断；翻页用 resume 游标继续。
    pub truncated: bool,
}

impl ScanResult {
    /// 还有后续内容可读（未到 EOF 且不是被取消）。
    pub fn has_more(&self) -> bool {
        !self.eof_reached && !self.cancelled
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    Eof,
    Deadline,
    Cancelled,
}

/// 分块行游标：产出 (行号, 行首偏移, 行字节)。
/// 不变量：`pos` 始终停在行首（或文件尾），`line_no` 是该行的行号。
struct LineCursor<'a> {
    data: &'a [u8],
    pos: usize,
    line_no: u64,
    next_check: usize,
    deadline: Instant,
    cancel: Option<&'a AtomicBool>,
    stop: Option<Stop>,
}

impl<'a> LineCursor<'a> {
    fn new(
        data: &'a [u8],
        start_offset: u64,
        start_line: u64,
        time_budget: Duration,
        cancel: Option<&'a AtomicBool>,
    ) -> Self {
        let pos = (start_offset as usize).min(data.len());
        let mut c = Self {
            data,
            pos,
            line_no: start_line,
            next_check: usize::MAX,
            deadline: Instant::now() + time_budget,
            cancel,
            stop: None,
        };
        c.next_check = data.len().min(pos.saturating_add(CHUNK_SIZE));
        c
    }

    fn next_line(&mut self) -> Option<(u64, u64, &'a [u8])> {
        if self.stop.is_some() {
            return None;
        }
        let len = self.data.len();
        if self.pos >= len {
            self.stop = Some(Stop::Eof);
            return None;
        }
        // 首行前也检查一次，保证 time_budget=0 或预先置位的取消立即生效。
        if self.pos >= self.next_check || self.pos == 0 {
            if Instant::now() >= self.deadline {
                self.stop = Some(Stop::Deadline);
                return None;
            }
            if let Some(c) = self.cancel {
                if c.load(Ordering::Relaxed) {
                    self.stop = Some(Stop::Cancelled);
                    return None;
                }
            }
            self.next_check = len.min(self.pos.saturating_add(CHUNK_SIZE));
        }
        let offset = self.pos;
        let line_no = self.line_no;
        let rest = &self.data[self.pos..];
        let mut end = match memchr(b'\n', rest) {
            Some(nl) => {
                self.pos += nl + 1;
                nl
            }
            None => {
                self.pos = len;
                rest.len()
            }
        };
        if end > 0 && rest[end - 1] == b'\r' {
            end -= 1;
        }
        self.line_no += 1;
        Some((line_no, offset as u64, &rest[..end]))
    }
}

/// ASCII 大小写不敏感子串匹配（零分配，与 detector 的匹配语义一致）。
/// 用首字节的两个大小写变体做 memchr SIMD 预筛，命中处再做窗口校验。
fn ascii_contains_ci(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    let first = needle[0];
    let first_alt = if first.is_ascii_alphabetic() {
        if first.is_ascii_lowercase() {
            first.to_ascii_uppercase()
        } else {
            first.to_ascii_lowercase()
        }
    } else {
        first
    };
    let two = [first, first_alt];
    let variants: &[u8] = if two[1] == two[0] { &two[..1] } else { &two };
    let last = haystack.len() - needle.len();
    for v in variants {
        for pos in memchr::memchr_iter(*v, &haystack[..=last]) {
            if haystack[pos..pos + needle.len()]
                .iter()
                .zip(needle.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                return true;
            }
        }
    }
    false
}

/// AND 关键字匹配器。关键字预处理为小写；行匹配复用 `ascii_contains_ci`
/// （内部有首字节 memchr SIMD 预筛）。
struct Matcher {
    needles_lower: Vec<Vec<u8>>,
}

impl Matcher {
    fn new(keywords: &[String]) -> Self {
        let needles_lower: Vec<Vec<u8>> = keywords
            .iter()
            .map(|k| k.as_bytes().to_ascii_lowercase())
            .filter(|k| !k.is_empty())
            .collect();
        Self { needles_lower }
    }

    /// 空关键字列表 = 不过滤（所有行都算命中）。
    fn matches(&self, line: &[u8]) -> bool {
        if self.needles_lower.is_empty() {
            return true;
        }
        self.needles_lower
            .iter()
            .all(|n| ascii_contains_ci(line, n))
    }
}

/// 尚未物化为 String 的原始行引用：借用 mmap，窗口关闭时才拷贝文本，
/// 保证扫描热循环零字符串分配。
struct RawEntry {
    line_no: u64,
    offset: u64,
    len: usize,
    is_match: bool,
}

struct RawWindow {
    entries: Vec<RawEntry>,
    end_line: u64,
    after_left: usize,
    bytes_out: usize,
}

impl RawWindow {
    fn push(&mut self, line_no: u64, offset: u64, len: usize, is_match: bool) {
        self.bytes_out += len + 1;
        self.end_line = line_no;
        self.entries.push(RawEntry {
            line_no,
            offset,
            len,
            is_match,
        });
    }
}

/// 向后补种的上限窗口。超长行（> 256KB 无换行）在补种时被放弃——
/// 避免为找 context_before 个换行而反向扫描整个文件。
const BACKSCAN_LIMIT: usize = 256 * 1024;

/// 向后取 `count` 条完整行（行首偏移 `start`、行号 `start_line` 的前文），
/// 用于 resume 时补种 before-context 环。返回按行号升序。
/// `start` 必须在行首（即 data[start-1] == '\n'），否则返回空。
fn seed_backwards(
    data: &[u8],
    start: usize,
    start_line: u64,
    count: usize,
) -> VecDeque<(u64, u64, usize)> {
    let mut out = VecDeque::new();
    if count == 0 || start == 0 || start_line <= 1 {
        return out;
    }
    if data[start - 1] != b'\n' {
        return out; // 非行首游标，拒绝补种
    }
    let mut line_end = start - 1; // 上一行的结尾（不含换行）
    let mut line_no = start_line - 1;
    for _ in 0..count {
        let window_start = line_end.saturating_sub(BACKSCAN_LIMIT);
        let (offset, mut end) = match memrchr(b'\n', &data[window_start..line_end]) {
            Some(nl) => (window_start + nl + 1, line_end),
            None => {
                if window_start > 0 {
                    break; // 窗口内找不到换行：行超长或跨出限制，放弃补种
                }
                (0, line_end)
            }
        };
        if end > offset && data[end - 1] == b'\r' {
            end -= 1;
        }
        out.push_front((line_no, offset as u64, end - offset));
        if offset == 0 {
            break;
        }
        line_end = offset - 1;
        if line_no <= 1 {
            break;
        }
        line_no -= 1;
    }
    out
}

/// 扫描整个文件。`cancel` 由调用方持有（Web 层的 Drop guard 在客户端断开时置位）。
pub fn scan_file(
    path: &Path,
    params: &ScanParams,
    cancel: Option<&AtomicBool>,
) -> io::Result<ScanResult> {
    let file = File::open(path)?;
    // 文件在映射期间被截断会触发 SIGBUS——与 tail-engine 的 LineIndex 接受同样的
    // 风险边界（tailr 只暴露配置目录下的本地日志文件）。
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(scan_bytes(&mmap, params, cancel))
}

fn scan_bytes(data: &[u8], params: &ScanParams, cancel: Option<&AtomicBool>) -> ScanResult {
    let mut cursor = LineCursor::new(
        data,
        params.start_offset,
        params.start_line,
        params.time_budget,
        cancel,
    );
    let matcher = Matcher::new(&params.keywords);

    // before-context 环：存原始引用，容量 context_before。
    let mut ring: VecDeque<(u64, u64, usize)> = seed_backwards(
        data,
        (params.start_offset as usize).min(data.len()),
        params.start_line,
        params.context_before,
    );

    let mut windows: Vec<RawWindow> = Vec::new();
    let mut open: Option<RawWindow> = None;
    let mut matched: u64 = 0;
    let mut truncated = false;

    while let Some((line_no, offset, line)) = cursor.next_line() {
        let len = line.len();
        let is_match = matcher.matches(line);

        if params.count_only {
            if is_match {
                matched += 1;
                if matched as usize >= params.max_matches {
                    truncated = true;
                    break;
                }
            }
            continue;
        }

        if is_match {
            matched += 1;
        }

        match open.as_mut() {
            Some(w) => {
                if is_match {
                    // 合并间隔行：环中行号大于窗口末行的即是未入窗的 gap 行
                    // （after-context 行入窗时已更新 end_line，会被过滤跳过）。
                    let gap: Vec<_> = ring
                        .iter()
                        .filter(|(rn, _, _)| *rn > w.end_line)
                        .copied()
                        .collect();
                    for (gn, go, gl) in gap {
                        w.push(gn, go, gl, false);
                    }
                    w.push(line_no, offset, len, true);
                    w.after_left = params.context_after;
                } else if w.after_left > 0 {
                    w.push(line_no, offset, len, false);
                    w.after_left -= 1;
                } else {
                    let done = open.take().expect("checked open above");
                    windows.push(done);
                }
            }
            None => {
                if is_match {
                    let mut w = RawWindow {
                        entries: Vec::new(),
                        end_line: 0,
                        after_left: params.context_after,
                        bytes_out: 0,
                    };
                    for (bn, bo, bl) in ring.iter().copied() {
                        w.push(bn, bo, bl, false);
                    }
                    w.push(line_no, offset, len, true);
                    open = Some(w);
                }
            }
        }

        // 每行无条件入环（容量 context_before）。已入窗的行在后续合并时被
        // `rn > end_line` 过滤，不会重复；统一入环避免分支间的双入队 bug。
        push_ring(&mut ring, line_no, offset, len, params.context_before);

        // 预算检查：命中只会让 open 变 Some（追加或新开窗口），此处必然有窗口。
        if let Some(w) = open.as_ref() {
            if matched as usize >= params.max_matches || w.bytes_out >= params.max_bytes {
                truncated = true;
                break;
            }
        }
    }

    if let Some(w) = open.take() {
        windows.push(w);
    }

    let timed_out = cursor.stop == Some(Stop::Deadline);
    let cancelled = cursor.stop == Some(Stop::Cancelled);
    let eof_reached = cursor.stop == Some(Stop::Eof) && !truncated;

    let start_line = params.start_line;
    ScanResult {
        windows: windows
            .into_iter()
            .map(|w| materialize_window(data, w, params.max_line_bytes))
            .collect(),
        matched_lines: matched,
        lines_scanned: cursor.line_no.saturating_sub(start_line),
        resume_offset: cursor.pos as u64,
        resume_line: cursor.line_no,
        eof_reached,
        timed_out,
        cancelled,
        truncated,
    }
}

fn push_ring(
    ring: &mut VecDeque<(u64, u64, usize)>,
    line_no: u64,
    offset: u64,
    len: usize,
    cap: usize,
) {
    if cap == 0 {
        return;
    }
    ring.push_back((line_no, offset, len));
    while ring.len() > cap {
        ring.pop_front();
    }
}

fn materialize_window(data: &[u8], w: RawWindow, max_line_bytes: usize) -> ContextWindow {
    let start_line = w.entries.first().map(|e| e.line_no).unwrap_or(0);
    let end_line = w.end_line;
    let entries = w
        .entries
        .into_iter()
        .map(|e| {
            let raw = &data[e.offset as usize..(e.offset as usize + e.len)];
            let (text, line_truncated) = if raw.len() > max_line_bytes {
                let mut s = String::from_utf8_lossy(&raw[..max_line_bytes]).into_owned();
                s.push_str(LINE_TRUNCATION_MARKER);
                (s, true)
            } else {
                (String::from_utf8_lossy(raw).into_owned(), false)
            };
            LineEntry {
                line_no: e.line_no,
                offset: e.offset,
                text,
                line_truncated,
                is_match: e.is_match,
            }
        })
        .collect();
    ContextWindow {
        start_line,
        end_line,
        entries,
    }
}

/// 文件统计（get_log_stats 的地基）：行数、字节数、按级别计数。
/// 同样受时间预算与取消约束；未扫完时 `completed=false`（部分计数）。
#[derive(Debug, Clone)]
pub struct FileStats {
    pub total_lines: u64,
    pub total_bytes: u64,
    /// (级别名, 行数)，按 LevelDetector 配置顺序。
    pub levels: Vec<(String, u64)>,
    pub unknown_lines: u64,
    pub completed: bool,
}

pub fn file_stats(
    path: &Path,
    detector: Option<&crate::LevelDetector>,
    time_budget: Duration,
    cancel: Option<&AtomicBool>,
) -> io::Result<FileStats> {
    let file = File::open(path)?;
    let total_bytes = file.metadata()?.len();
    let mmap = unsafe { Mmap::map(&file)? };
    let mut cursor = LineCursor::new(&mmap, 0, 1, time_budget, cancel);

    let mut counts: Vec<(String, u64)> = detector
        .map(|d| d.level_names().into_iter().map(|n| (n.to_string(), 0)).collect())
        .unwrap_or_default();
    let mut unknown: u64 = 0;
    let mut total_lines: u64 = 0;

    while let Some((_, _, line)) = cursor.next_line() {
        total_lines += 1;
        if let Some(d) = detector {
            let text = String::from_utf8_lossy(line);
            let level = d.detect_ref(&text);
            match counts.iter_mut().find(|(n, _)| n == level) {
                Some((_, c)) => *c += 1,
                None => unknown += 1, // detect_ref 的 "UNKNOWN"
            }
        }
    }

    let completed = cursor.stop == Some(Stop::Eof);
    Ok(FileStats {
        total_lines,
        total_bytes,
        levels: counts,
        unknown_lines: unknown,
        completed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_log(content: &str) -> (tempfile::NamedTempFile, std::path::PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let path = f.path().to_path_buf();
        (f, path)
    }

    fn params(keywords: &[&str]) -> ScanParams {
        ScanParams {
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            ..ScanParams::default()
        }
    }

    fn flat_entries(r: &ScanResult) -> Vec<&LineEntry> {
        r.windows.iter().flat_map(|w| w.entries.iter()).collect()
    }

    #[test]
    fn and_match_case_insensitive() {
        let (_f, path) = write_log(
            "2026-08-14 10:00:01 INFO started\n\
             2026-08-14 10:00:02 ERROR DB pool exhausted\n\
             2026-08-14 10:00:03 error other thing\n\
             2026-08-14 10:00:04 error db pool exhausted again\n",
        );
        let r = scan_file(&path, &params(&["error", "db"]), None).unwrap();
        let all = flat_entries(&r);
        let matches: Vec<_> = all.iter().filter(|e| e.is_match).collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_no, 2);
        assert_eq!(matches[1].line_no, 4);
        assert!(r.eof_reached);
        assert!(!r.truncated);
    }

    #[test]
    fn adjacent_matches_merge_into_one_window() {
        let content: String = (0..10)
            .map(|i| format!("line {i} {}\n", if i == 4 || i == 5 || i == 6 { "ERR" } else { "ok" }))
            .collect();
        let (_f, path) = write_log(&content);
        // 默认 context 2/2：match@5、6、7 连续合并，尾部 after-context 到 line 9。
        let r = scan_file(&path, &params(&["ERR"]), None).unwrap();
        assert_eq!(r.windows.len(), 1);
        assert_eq!(r.windows[0].start_line, 3);
        assert_eq!(r.windows[0].end_line, 9);
        assert_eq!(r.matched_lines, 3);
    }

    #[test]
    fn distant_matches_get_separate_windows_with_context() {
        let content: String = (0..20)
            .map(|i| format!("line {i} {}\n", if i == 2 || i == 15 { "ERR" } else { "ok" }))
            .collect();
        let (_f, path) = write_log(&content);
        let r = scan_file(&path, &params(&["ERR"]), None).unwrap();
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.windows[0].start_line, 1); // before-context 到文件头截断
        assert_eq!(r.windows[0].end_line, 5);
        assert_eq!(r.windows[1].start_line, 14);
        assert_eq!(r.windows[1].end_line, 18);
    }

    #[test]
    fn resume_continues_without_duplicates_and_backfills_context() {
        let content: String = (0..12)
            .map(|i| format!("line {i:02} {}\n", if i % 4 == 0 { "ERR" } else { "ok" }))
            .collect();
        let (_f, path) = write_log(&content);

        let mut p = params(&["ERR"]);
        p.max_matches = 1; // 第 1 次命中（line 1）后即截断
        let r1 = scan_file(&path, &p, None).unwrap();
        assert!(r1.truncated);
        assert_eq!(r1.matched_lines, 1);
        assert_eq!(r1.windows[0].entries.iter().filter(|e| e.is_match).count(), 1);

        // 续扫：游标续在 line 5（line 4 的 after-context 已发完）
        let mut p2 = params(&["ERR"]);
        p2.start_offset = r1.resume_offset;
        p2.start_line = r1.resume_line;
        let r2 = scan_file(&path, &p2, None).unwrap();
        let all2 = flat_entries(&r2);
        let matches2: Vec<_> = all2.iter().filter(|e| e.is_match).collect();
        assert!(matches2.iter().all(|e| e.line_no > 4));
        // 反向补种：line 5 的命中带 line 3-4 的 before-context
        assert_eq!(r2.windows[0].start_line, 3);
        assert!(r2.eof_reached);
    }

    #[test]
    fn max_bytes_budget_truncates() {
        let content: String = (0..100)
            .map(|i| format!("line {i:03} padding-padding-padding ERR\n"))
            .collect();
        let (_f, path) = write_log(&content);
        let mut p = params(&["ERR"]);
        p.max_bytes = 300;
        let r = scan_file(&path, &p, None).unwrap();
        assert!(r.truncated);
        assert!(!r.eof_reached);
        let all = flat_entries(&r);
        let out_bytes: usize = all.iter().map(|e| e.text.len() + 1).sum();
        assert!(out_bytes < 700); // 截断发生在略超预算的窗口边界
        // 续扫能继续
        let mut p2 = params(&["ERR"]);
        p2.start_offset = r.resume_offset;
        p2.start_line = r.resume_line;
        assert!(scan_file(&path, &p2, None).unwrap().matched_lines > 0);
    }

    #[test]
    fn zero_time_budget_stops_immediately() {
        let (_f, path) = write_log("a ERR\nb ERR\nc ERR\n");
        let mut p = params(&["ERR"]);
        p.time_budget = Duration::ZERO;
        let r = scan_file(&path, &p, None).unwrap();
        assert!(r.timed_out);
        assert_eq!(r.matched_lines, 0);
        assert_eq!(r.resume_offset, 0);
        assert_eq!(r.resume_line, 1);
    }

    #[test]
    fn cancel_flag_stops_before_first_line() {
        let (_f, path) = write_log("a ERR\n");
        let flag = AtomicBool::new(true);
        let r = scan_file(&path, &params(&["ERR"]), Some(&flag)).unwrap();
        assert!(r.cancelled);
        assert_eq!(r.matched_lines, 0);
    }

    #[test]
    fn crlf_and_missing_trailing_newline() {
        let (_f, path) = write_log("one ERR\r\ntwo ERR\r\nthree ERR");
        let r = scan_file(&path, &params(&["ERR"]), None).unwrap();
        assert_eq!(r.matched_lines, 3);
        assert!(r.eof_reached);
        let all = flat_entries(&r);
        let texts: Vec<&str> = all.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts[0], "one ERR"); // \r 已剥离
        assert_eq!(texts[2], "three ERR"); // 无尾换行的末行
    }

    #[test]
    fn long_line_is_truncated_with_marker() {
        let long_line = format!("ERR {}", "x".repeat(10_000));
        let (_f, path) = write_log(&long_line);
        let r = scan_file(&path, &params(&["ERR"]), None).unwrap();
        let all = flat_entries(&r);
        let e = all[0];
        assert!(e.line_truncated);
        assert!(e.text.ends_with(LINE_TRUNCATION_MARKER));
        assert!(e.text.len() < 2 * DEFAULT_MAX_LINE_BYTES);
    }

    #[test]
    fn empty_keywords_match_every_line() {
        let (_f, path) = write_log("a\nb\nc\n");
        let r = scan_file(&path, &params(&[]), None).unwrap();
        assert_eq!(r.matched_lines, 3);
        assert_eq!(r.windows.len(), 1); // 连续命中合并
    }

    #[test]
    fn empty_file() {
        let (_f, path) = write_log("");
        let r = scan_file(&path, &params(&["ERR"]), None).unwrap();
        assert!(r.eof_reached);
        assert_eq!(r.matched_lines, 0);
        assert_eq!(r.lines_scanned, 0);
    }

    #[test]
    fn start_offset_mid_file_counts_lines_correctly() {
        let content: String = (0..10)
            .map(|i| format!("line {i} {}\n", if i == 6 { "ERR" } else { "ok" }))
            .collect();
        let (_f, path) = write_log(&content);
        // 手工计算 line 4 的行首偏移（"line 0 ok\n" = 10 字节/行，行等长）
        let line4_offset = 3 * 10;
        let mut p = params(&["ERR"]);
        p.start_offset = line4_offset;
        p.start_line = 4;
        let r = scan_file(&path, &p, None).unwrap();
        let all = flat_entries(&r);
        let m: Vec<_> = all.iter().filter(|e| e.is_match).collect();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].line_no, 7); // "line 6 ERR"
        assert_eq!(r.lines_scanned, 7); // line 4..10
    }

    #[test]
    fn count_only_counts_without_windows_and_pages() {
        let content: String = (0..30)
            .map(|i| format!("line {i:02} {}\n", if i % 3 == 0 { "HIT" } else { "ok" }))
            .collect();
        let (_f, path) = write_log(&content);

        // 第一页：数到 max_matches=5 截断，无窗口输出
        let mut p = params(&["HIT"]);
        p.count_only = true;
        p.max_matches = 5;
        let r1 = scan_file(&path, &p, None).unwrap();
        assert!(r1.truncated);
        assert_eq!(r1.matched_lines, 5);
        assert!(r1.windows.is_empty());

        // 续页数完：30 行 / 3 = 10 个命中
        let mut p2 = params(&["HIT"]);
        p2.count_only = true;
        p2.max_matches = 1000;
        p2.start_offset = r1.resume_offset;
        p2.start_line = r1.resume_line;
        let r2 = scan_file(&path, &p2, None).unwrap();
        assert!(r2.eof_reached);
        assert_eq!(r1.matched_lines + r2.matched_lines, 10);
    }

    #[test]
    fn stats_counts_lines_and_levels() {
        let detector = crate::LevelDetector::from_config(&tailr_protocol::LogLevelConfig {
            preset: "default".to_string(),
            levels: vec![tailr_protocol::LevelDef {
                name: "ERROR".to_string(),
                keywords: vec!["ERROR".to_string()],
                color_light: "#000000".to_string(),
                color_dark: "#ffffff".to_string(),
            }],
        });
        let (_f, path) = write_log(
            "2026-08-14 10:00:01 INFO started\n\
             2026-08-14 10:00:02 ERROR db refused\n\
             plain line without level\n",
        );
        let s = file_stats(&path, Some(&detector), DEFAULT_TIME_BUDGET, None).unwrap();
        assert!(s.completed);
        assert_eq!(s.total_lines, 3);
        let err = s.levels.iter().find(|(n, _)| n == "ERROR").unwrap();
        assert_eq!(err.1, 1);
        // 只配置了 ERROR 级别：INFO 行与无级别行都归 UNKNOWN。
        assert_eq!(s.unknown_lines, 2);
    }

    /// 性能冒烟（不进常规测试）：`cargo test -p tailr-search-engine --release perf_smoke -- --ignored --nocapture`
    /// 200MB 全量扫描（热缓存）应在亚秒级；对应 MCP 验收标准 2GB < 1s 的量级验证。
    #[test]
    #[ignore]
    fn perf_smoke_200mb_scan() {
        let target_bytes = 200 * 1024 * 1024;
        let line = "2026-08-14 10:00:00.123 INFO  order-service handled request id=1234567 status=200\n";
        let lines_needed = target_bytes / line.len();
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let mut written = 0usize;
        for i in 0..lines_needed {
            if i % 100_000 == 0 {
                let hit = "2026-08-14 10:00:00.123 ERROR payment gateway refused card\n";
                write!(f, "{hit}").unwrap();
                written += hit.len();
            }
            write!(f, "{line}").unwrap();
            written += line.len();
        }
        f.flush().unwrap();

        let start = Instant::now();
        let r = scan_file(f.path(), &params(&["refused", "card"]), None).unwrap();
        let elapsed = start.elapsed();
        let mbps = written as f64 / 1024.0 / 1024.0 / elapsed.as_secs_f64();
        println!(
            "scanned {} MB in {:?} ({:.0} MB/s), matched {}",
            written / 1024 / 1024,
            elapsed,
            mbps,
            r.matched_lines
        );
        assert!(r.eof_reached);
        assert!(r.matched_lines >= 2);
        // 宽松下限：低于 500 MB/s 无法满足 2GB<1s 验收，说明实现退化。
        assert!(mbps > 500.0, "throughput too low: {mbps} MB/s");
    }
}

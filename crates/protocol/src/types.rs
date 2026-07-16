use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Timezone assumption used when parsing timezone-less (naive) log timestamps.
/// Ctime lines with an explicit offset (e.g. `+08`) always use that offset;
/// this config only applies to naive timestamps.
#[derive(Debug, Clone, Default)]
pub enum LogTimezone {
    /// Server local timezone (default, backward compatible).
    #[default]
    Local,
    /// UTC.
    Utc,
    /// Fixed UTC offset (e.g. +08:00).
    Fixed(FixedOffset),
}

impl LogTimezone {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        match s.to_ascii_lowercase().as_str() {
            "local" | "" => Ok(LogTimezone::Local),
            "utc" | "z" => Ok(LogTimezone::Utc),
            _ => {
                let off = Self::parse_offset(s).ok_or_else(|| {
                    format!("invalid timezone '{}': expected local|utc|+HH:MM|+HHMM", s)
                })?;
                Ok(LogTimezone::Fixed(off))
            }
        }
    }

    /// Parse an offset token like `+08`, `+0800`, `+08:00`, `-05:00`. Used by ctime parser.
    pub(crate) fn parse_offset(s: &str) -> Option<FixedOffset> {
        let (sign, rest) = match s.chars().next()? {
            '+' => (1i32, &s[1..]),
            '-' => (-1i32, &s[1..]),
            _ => return None,
        };
        let (h, m) = if let Some((hh, mm)) = rest.split_once(':') {
            (hh.parse::<i32>().ok()?, mm.parse::<i32>().ok()?)
        } else if rest.len() == 4 {
            (
                rest[..2].parse::<i32>().ok()?,
                rest[2..].parse::<i32>().ok()?,
            )
        } else if rest.len() == 2 {
            (rest.parse::<i32>().ok()?, 0)
        } else {
            return None;
        };
        // Valid offsets range from -12:00 to +14:00; clamp hour to ±14, minutes to 0..=59.
        if !(0..=14).contains(&h) || !(0..=59).contains(&m) {
            return None;
        }
        FixedOffset::east_opt(sign * (h * 3600 + m * 60))
    }

    fn naive_to_utc(&self, dt: &NaiveDateTime) -> Option<DateTime<Utc>> {
        match self {
            LogTimezone::Local => Local
                .from_local_datetime(dt)
                .earliest()
                .map(|l: DateTime<Local>| l.with_timezone(&Utc)),
            LogTimezone::Utc => Some(Utc.from_utc_datetime(dt)),
            LogTimezone::Fixed(off) => off
                .from_local_datetime(dt)
                .earliest()
                .map(|d: DateTime<FixedOffset>| d.with_timezone(&Utc)),
        }
    }
}

// ── 日志级别配置 ──────────────────────────────────────────

/// 单个日志级别定义（名称 + 检测关键词 + 颜色）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelDef {
    /// 级别名称（如 "ERROR", "CRITICAL"）
    pub name: String,
    /// 检测关键词（如 ["ERROR", "ERR"]），大小写不敏感
    pub keywords: Vec<String>,
    /// 浅色主题颜色（HEX）
    pub color_light: String,
    /// 深色主题颜色（HEX）
    pub color_dark: String,
}

/// 日志级别配置（预设名称 + 级别列表）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLevelConfig {
    /// 预设名称（"general" | "java" | "python" | "php" | "go" | "rust" | "syslog" | "custom"）
    pub preset: String,
    /// 级别列表，顺序 = 检测优先级
    pub levels: Vec<LevelDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    ALERT,
    ERROR,
    WARN,
    INFO,
    DEBUG,
    TRACE,
    UNKNOWN,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub line_num: u64,
    pub raw: String,
    /// 日志级别名称（动态，支持自定义级别如 "CRITICAL"、"FATAL"）
    pub level: String,
    /// 解析后的 UTC 时间戳（用于排序/过滤）
    pub timestamp: Option<DateTime<Utc>>,
    /// 原始日志中的时间文本子串（用于前端精确显示，不做时区转换）
    pub raw_timestamp: Option<String>,
    pub fields: Option<serde_json::Value>,
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    let haystack_bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return true;
    }
    if haystack_bytes.len() < needle_bytes.len() {
        return false;
    }
    let limit = haystack_bytes.len().min(256);
    for i in 0..=limit - needle_bytes.len() {
        if haystack_bytes[i..i + needle_bytes.len()]
            .iter()
            .zip(needle_bytes.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return true;
        }
    }
    false
}

pub fn detect_level(line: &str) -> LogLevel {
    if contains_case_insensitive(line, "ALERT") || contains_case_insensitive(line, "[ALERT]") {
        LogLevel::ALERT
    } else if contains_case_insensitive(line, "ERROR") || contains_case_insensitive(line, "[ERROR]") || contains_case_insensitive(line, " E ") {
        LogLevel::ERROR
    } else if contains_case_insensitive(line, "WARN") || contains_case_insensitive(line, "[WARN]") || contains_case_insensitive(line, " W ") {
        LogLevel::WARN
    } else if contains_case_insensitive(line, "INFO") || contains_case_insensitive(line, "[INFO]") || contains_case_insensitive(line, " I ") {
        LogLevel::INFO
    } else if contains_case_insensitive(line, "DEBUG") || contains_case_insensitive(line, "[DEBUG]") || contains_case_insensitive(line, " D ") {
        LogLevel::DEBUG
    } else if contains_case_insensitive(line, "TRACE") || contains_case_insensitive(line, "[TRACE]") {
        LogLevel::TRACE
    } else {
        LogLevel::UNKNOWN
    }
}

pub fn try_parse_timestamp(
    line: &str,
    tz: &LogTimezone,
) -> (Option<DateTime<Utc>>, Option<String>) {
    if line.starts_with('[') {
        if let Some(end) = line.find(']') {
            let inner = line[1..end].trim();
            let result = try_parse_timestamp(inner, tz);
            if result.0.is_some() || result.1.is_some() {
                return result;
            }
        }
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(line.get(..30).unwrap_or(line)) {
        return (Some(dt.with_timezone(&Utc)), None);
    }

    if let Some((ts, raw)) = try_parse_ctime(line, tz) {
        return (Some(ts), Some(raw));
    }

    let patterns: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.3f",
        "%Y-%m-%d %H:%M:%S",
        "%d/%b/%Y:%H:%M:%S",
    ];

    for pattern in patterns {
        let target_len = match *pattern {
            "%Y-%m-%d %H:%M:%S%.3f" => 23,
            "%Y-%m-%d %H:%M:%S" => 19,
            "%d/%b/%Y:%H:%M:%S" => 26,
            _ => pattern.len(),
        };
        let end = target_len.min(line.len());
        let slice = line.get(..end);
        if let Some(slice) = slice {
            let trimmed = slice.trim();
            if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, pattern) {
                let utc = tz.naive_to_utc(&dt);
                return (utc, Some(trimmed.to_string()));
            }
        }
    }

    if let Some((ts, raw)) = try_parse_date_only(line, tz) {
        return (Some(ts), Some(raw));
    }

    if let Some((ts, raw)) = try_parse_time_only(line) {
        return (ts, Some(raw));
    }

    // Unix epoch: look for a standalone number like 1764518400.1775
    // Scan for sequences of digits (10+ digits) optionally followed by a decimal fraction
    let mut start = 0;
    let bytes = line.as_bytes();
    while start < bytes.len() {
        if bytes[start].is_ascii_digit() && (start == 0 || !bytes[start - 1].is_ascii_digit()) {
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            // Check for decimal fraction
            let frac_end = if end < bytes.len() && bytes[end] == b'.' {
                let mut fe = end + 1;
                while fe < bytes.len() && bytes[fe].is_ascii_digit() {
                    fe += 1;
                }
                fe
            } else {
                end
            };
            // Must be 10+ integer digits (seconds since ~2001) and not followed by a digit
            if end - start >= 10 {
                let num_str = std::str::from_utf8(&bytes[start..frac_end]).unwrap_or("");
                if let Ok(secs) = num_str.parse::<f64>() {
                    if secs > 1_000_000_000.0 && secs < 2_000_000_000.0 {
                        let secs_int = secs.floor() as i64;
                        let nanos = (secs.fract() * 1_000_000_000.0) as u32;
                        let ts = DateTime::from_timestamp(secs_int, nanos);
                        if let Some(utc) = ts {
                            let display = utc.with_timezone(&Local).format("%H:%M:%S%.3f").to_string();
                            return (Some(utc), Some(display));
                        }
                    }
                }
            }
            start = frac_end;
        } else {
            start += 1;
        }
    }

    // Fallback: extract timestamp from JSON fields (e.g. {"time":"2026-07-05 17:51:33"})
    if let Some((ts, raw)) = try_ts_from_json(line, tz) {
        return (Some(ts), Some(raw));
    }

    (None, None)
}

/// ctime/asctime: `Sun Jul  5 22:43:21 2026` or `... +08 2026`. The naive
/// datetime is converted to UTC via `tz` when no offset is present; an
/// explicit offset (e.g. `+08`) always wins.
fn try_parse_ctime(line: &str, tz: &LogTimezone) -> Option<(DateTime<Utc>, String)> {
    const WEEKDAYS: &[&str] = &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    let head = line.get(..line.len().min(64))?;
    let mut tokens = head.split_whitespace();

    let weekday = tokens.next()?;
    if weekday.len() != 3 || !WEEKDAYS.contains(&weekday) {
        return None;
    }
    let month = tokens.next()?;
    if month.len() != 3 || !month.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let day: u32 = tokens.next()?.split('.').next()?.parse().ok()?;
    let time = tokens.next()?;
    let mut parts = time.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let min: u32 = parts.next()?.parse().ok()?;
    let sec: u32 = parts.next()?.split('.').next()?.parse().ok()?;

    let next = tokens.next()?;
    let (explicit_offset, year_token) = if next.starts_with('+') || next.starts_with('-') {
        let off = LogTimezone::parse_offset(next)?;
        let yr = tokens.next()?;
        (Some(off), yr)
    } else {
        (None, next)
    };
    let year_num: i32 = year_token.parse().ok()?;

    let month_num = month_to_num(month)?;
    let date = NaiveDate::from_ymd_opt(year_num, month_num, day)?;
    let dt = date.and_hms_opt(hour, min, sec)?;
    let utc = if let Some(off) = explicit_offset {
        off.from_local_datetime(&dt)
            .earliest()
            .map(|d: DateTime<FixedOffset>| d.with_timezone(&Utc))?
    } else {
        tz.naive_to_utc(&dt)?
    };

    let raw = format_raw_components(year_num, month_num, day, hour, min, sec, 0);
    Some((utc, raw))
}

fn month_to_num(m: &str) -> Option<u32> {
    match m {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

/// Date-only prefix `2026-07-05 <content>`. The separator after the date must
/// be a space/tab/EOL — an ISO `T` is rejected so `2026-07-05T...` isn't
/// silently truncated to a date.
fn try_parse_date_only(line: &str, tz: &LogTimezone) -> Option<(DateTime<Utc>, String)> {
    let bytes = line.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let slice = line.get(..10)?;
    let date = NaiveDate::parse_from_str(slice, "%Y-%m-%d").ok()?;
    match bytes.get(10).copied() {
        None | Some(b' ') | Some(b'\t') => {}
        _ => return None,
    }
    let dt = date.and_hms_opt(0, 0, 0)?;
    let utc = tz.naive_to_utc(&dt)?;
    let raw = format_raw_components(date.year(), date.month(), date.day(), 0, 0, 0, 0);
    Some((utc, raw))
}

fn format_raw_components(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    min: u32,
    sec: u32,
    millis: u32,
) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        year, month, day, hour, min, sec, millis
    )
}

/// Time-only prefix `HH:MM:SS` or `HH:MM:SS.fff`. The date is unknown, so no
/// sortable `DateTime` is produced (returns `None`) and the raw is rendered
/// with a `0000-00-00` date placeholder.
fn try_parse_time_only(line: &str) -> Option<(Option<DateTime<Utc>>, String)> {
    let bytes = line.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    let hour: u32 = line.get(..2)?.parse().ok()?;
    let min: u32 = line.get(3..5)?.parse().ok()?;
    let sec: u32 = line.get(6..8)?.parse().ok()?;
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    let (millis, end) = if bytes.len() >= 12
        && bytes[8] == b'.'
        && bytes[9].is_ascii_digit()
        && bytes[10].is_ascii_digit()
        && bytes[11].is_ascii_digit()
    {
        (line.get(9..12)?.parse().ok()?, 12usize)
    } else {
        (0u32, 8usize)
    };
    match bytes.get(end).copied() {
        None | Some(b' ') | Some(b'\t') => {}
        _ => return None,
    }
    let raw = format_raw_components(0, 0, 0, hour, min, sec, millis);
    Some((None, raw))
}

fn parse_datetime_str(s: &str, tz: &LogTimezone) -> Option<(DateTime<Utc>, String)> {
    let s = s.trim();

    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some((dt.with_timezone(&Utc), s.to_string()));
    }

    const PATTERNS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.3f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.3f",
        "%Y-%m-%dT%H:%M:%S",
        "%d/%b/%Y:%H:%M:%S",
    ];

    for pattern in PATTERNS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, pattern) {
            if let Some(utc) = tz.naive_to_utc(&dt) {
                return Some((utc, s.to_string()));
            }
        }
    }

    None
}

/// "date" is deliberately excluded — it's typically date-only (e.g. "20260705"),
/// not a full timestamp, and would cause false matches.
const JSON_TS_KEYS: &[&str] = &[
    "@timestamp",
    "timestamp",
    "time",
    "datetime",
    "ts",
    "log_time",
    "created_at",
];

fn try_ts_from_json(line: &str, tz: &LogTimezone) -> Option<(DateTime<Utc>, String)> {
    let start = line.find('{')?;
    let json: serde_json::Value = serde_json::from_str(line.get(start..)?).ok()?;
    let obj = json.as_object()?;

    for key in JSON_TS_KEYS {
        let Some(value) = obj.get(*key) else {
            continue;
        };

        if let Some(s) = value.as_str() {
            if let Some(result) = parse_datetime_str(s, tz) {
                return Some(result);
            }
        }

        if let Some(n) = value.as_f64() {
            let (secs, nanos) = if n > 1e12 {
                ((n / 1000.0).floor() as i64, ((n % 1000.0) * 1_000_000.0) as u32)
            } else if n > 1e9 {
                (n.floor() as i64, (n.fract() * 1_000_000_000.0) as u32)
            } else {
                continue;
            };
            if let Some(utc) = DateTime::from_timestamp(secs, nanos) {
                return Some((utc, value.to_string()));
            }
        }
    }

    None
}

pub fn try_parse_json_fields(line: &str) -> Option<serde_json::Value> {
    if let Some(start) = line.find('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line[start..]) {
            return Some(val);
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WSMessage {
    #[serde(rename_all = "camelCase")]
    Subscribe {
        path: String,
        after_seq: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Unsubscribe {
        path: String,
    },
    Ping,
    #[serde(rename_all = "camelCase")]
    Subscribed {
        path: String,
        /// Exact total line count from LineIndex::build. The HTTP file_tail
        /// endpoint only estimates this (tail_start), so the frontend carries
        /// an estimated lineNum coordinate on first load; this value lets it
        /// correct those line numbers once the precise index is ready.
        total_lines: u64,
    },
    #[serde(rename_all = "camelCase")]
    Append {
        path: String,
        seq: u64,
        entries: Vec<LogEntry>,
    },
    #[serde(rename_all = "camelCase")]
    Catchup {
        path: String,
        entries: Vec<LogEntry>,
        last_seq: u64,
    },
    Truncate {
        path: String,
    },
    Delete {
        path: String,
    },
    Pong,
    /// Server-pushed notification that a newer release is available.
    /// Broadcast to all connected WS clients by the background update-check task.
    #[serde(rename_all = "camelCase")]
    UpdateAvailable {
        latest_version: String,
        current_version: String,
        release_url: String,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line_number: u64,
    pub offset: u64,
    pub content: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> (Option<DateTime<Utc>>, Option<String>) {
        try_parse_timestamp(line, &LogTimezone::Local)
    }

    #[test]
    fn test_json_time_field_space_format() {
        let line = r#"{"message":"success","status":200,"date":"20260705","time":"2026-07-05 17:51:33","cityInfo":{"city":"天津市"}}"#;
        let (ts, raw) = parse(line);
        assert!(ts.is_some(), "should extract timestamp from JSON time field");
        assert_eq!(raw.as_deref(), Some("2026-07-05 17:51:33"));
    }

    #[test]
    fn test_json_timestamp_iso() {
        let line = r#"{"level":"INFO","@timestamp":"2026-07-05T17:51:33Z","msg":"ok"}"#;
        let (ts, raw) = parse(line);
        assert!(ts.is_some());
        assert_eq!(raw.as_deref(), Some("2026-07-05T17:51:33Z"));
    }

    #[test]
    fn test_json_timestamp_epoch_seconds() {
        let line = r#"{"ts":1751725893,"msg":"hello"}"#;
        let (ts, _) = parse(line);
        assert!(ts.is_some());
    }

    #[test]
    fn test_json_timestamp_epoch_millis() {
        let line = r#"{"timestamp":1751725893000.0,"msg":"hello"}"#;
        let (ts, _) = parse(line);
        assert!(ts.is_some());
    }

    #[test]
    fn test_line_start_timestamp_still_works() {
        let line = "2026-07-05 17:51:33 INFO server started";
        let (ts, raw) = parse(line);
        assert!(ts.is_some());
        assert_eq!(raw.as_deref(), Some("2026-07-05 17:51:33"));
    }

    #[test]
    fn test_no_timestamp_returns_none() {
        let (ts, _) = parse("just a plain log line with no time");
        assert!(ts.is_none());
    }

    #[test]
    fn test_json_date_only_not_matched() {
        let line = r#"{"date":"20260705","msg":"no time here"}"#;
        let (ts, _) = parse(line);
        assert!(ts.is_none(), "date-only field should not be matched");
    }

    #[test]
    fn test_date_only_prefix() {
        let (ts, raw) = parse("2026-07-05 some log content here");
        assert!(ts.is_some(), "date-only prefix should be parsed");
        assert_eq!(raw.as_deref(), Some("2026-07-05 00:00:00.000"));
    }

    #[test]
    fn test_date_only_exact() {
        let (ts, raw) = parse("2026-07-05");
        assert!(ts.is_some());
        assert_eq!(raw.as_deref(), Some("2026-07-05 00:00:00.000"));
    }

    #[test]
    fn test_date_only_rejects_iso_t() {
        let (ts, _) = parse("2026-07-05T17:51:33 server started");
        assert!(ts.is_none(), "ISO-T date must not be truncated to date-only");
    }

    #[test]
    fn test_ctime_with_two_digit_offset() {
        let (ts, raw) = parse("Sun Jul  5 22:43:21 +08 2026 something happened");
        assert!(ts.is_some(), "ctime with +08 offset should parse");
        assert_eq!(raw.as_deref(), Some("2026-07-05 22:43:21.000"));
    }

    #[test]
    fn test_ctime_without_offset() {
        let (ts, raw) = parse("Mon Dec 15 10:30:00 2025 server started");
        assert!(ts.is_some());
        assert_eq!(raw.as_deref(), Some("2025-12-15 10:30:00.000"));
    }

    #[test]
    fn test_ctime_rejects_non_weekday() {
        let (ts, _) = parse("Run Jul 5 10:30:00 2025 not a real weekday");
        assert!(ts.is_none(), "non-weekday prefix must not be treated as ctime");
    }

    #[test]
    fn test_time_only() {
        let (ts, raw) = parse("22:43:21 INFO server started");
        assert!(ts.is_none(), "time-only has no sortable timestamp");
        assert_eq!(raw.as_deref(), Some("0000-00-00 22:43:21.000"));
    }

    #[test]
    fn test_time_only_with_millis() {
        let (ts, raw) = parse("22:43:21.123 DEBUG request handled");
        assert!(ts.is_none());
        assert_eq!(raw.as_deref(), Some("0000-00-00 22:43:21.123"));
    }

    #[test]
    fn test_time_only_rejects_invalid() {
        let (ts, _) = parse("99:99:99 bad time");
        assert!(ts.is_none(), "out-of-range time must not parse");
    }

    #[test]
    fn test_bracketed_timestamp() {
        let (ts, raw) = parse("[2026-07-05 12:30:08] [FATAL] something broke");
        assert!(ts.is_some(), "bracketed timestamp should parse");
        assert_eq!(raw.as_deref(), Some("2026-07-05 12:30:08"));
    }

    #[test]
    fn test_ctime_offset_produces_correct_utc() {
        let (ts, _) = try_parse_timestamp(
            "Sun Jul  5 22:43:21 +08 2026 something",
            &LogTimezone::Utc,
        );
        let utc = ts.expect("ctime with offset should parse");
        assert_eq!(utc.format("%H:%M:%S").to_string(), "14:43:21");
    }

    // ── LogTimezone::parse / Fixed-offset conversion ───────────────────────

    #[test]
    fn test_log_timezone_named_variants() {
        assert!(matches!(LogTimezone::parse("local"), Ok(LogTimezone::Local)));
        assert!(matches!(LogTimezone::parse(""), Ok(LogTimezone::Local)));
        assert!(matches!(LogTimezone::parse("UTC"), Ok(LogTimezone::Utc)));
        assert!(matches!(LogTimezone::parse("z"), Ok(LogTimezone::Utc)));
    }

    #[test]
    fn test_log_timezone_offset_formats() {
        // `+HH:MM`, `+HHMM`, `+HH`, and the negative side all parse to Fixed.
        assert!(matches!(LogTimezone::parse("+08:00"), Ok(LogTimezone::Fixed(_))));
        assert!(matches!(LogTimezone::parse("+0800"), Ok(LogTimezone::Fixed(_))));
        assert!(matches!(LogTimezone::parse("+08"), Ok(LogTimezone::Fixed(_))));
        assert!(matches!(LogTimezone::parse("-05:00"), Ok(LogTimezone::Fixed(_))));
    }

    #[test]
    fn test_log_timezone_rejects_invalid() {
        assert!(LogTimezone::parse("gmt").is_err());
        assert!(LogTimezone::parse("+15:00").is_err(), "hour out of clamp range");
        assert!(LogTimezone::parse("+08:60").is_err(), "minutes out of range");
        assert!(LogTimezone::parse("abc").is_err());
    }

    #[test]
    fn test_fixed_offset_naive_to_utc_round_trip() {
        // A naive timestamp interpreted under +08:00 should yield a UTC value
        // 8 hours earlier. This covers the LogTimezone::Fixed branch of
        // naive_to_utc, which has no dedicated test elsewhere.
        let tz = LogTimezone::parse("+08:00").unwrap();
        let (ts, _) = try_parse_timestamp("[2026-07-05 22:43:21] log line", &tz);
        let utc = ts.expect("naive timestamp should parse under Fixed offset");
        assert_eq!(utc.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-07-05 14:43:21");
    }

    #[test]
    fn test_fixed_offset_negative() {
        let tz = LogTimezone::parse("-05:00").unwrap();
        let (ts, _) = try_parse_timestamp("[2026-07-05 10:30:00] log line", &tz);
        let utc = ts.expect("naive timestamp should parse under negative offset");
        assert_eq!(utc.format("%H:%M:%S").to_string(), "15:30:00");
    }
}

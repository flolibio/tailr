use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

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

pub fn try_parse_timestamp(line: &str) -> (Option<DateTime<Utc>>, Option<String>) {
    if let Ok(dt) = DateTime::parse_from_rfc3339(line.get(..30).unwrap_or(line)) {
        return (Some(dt.with_timezone(&Utc)), None);
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
                let utc = Local.from_local_datetime(&dt).earliest().map(|l: DateTime<Local>| l.with_timezone(&Utc));
                return (utc, Some(trimmed.to_string()));
            }
        }
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

    (None, None)
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
    Subscribe {
        path: String,
        after_seq: Option<u64>,
    },
    Unsubscribe {
        path: String,
    },
    Ping,
    #[serde(rename_all = "camelCase")]
    Subscribed {
        path: String,
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

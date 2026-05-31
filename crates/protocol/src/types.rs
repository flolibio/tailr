use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub level: LogLevel,
    pub timestamp: Option<DateTime<Utc>>,
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
            .all(|(a, b)| a.to_ascii_uppercase() == b.to_ascii_uppercase())
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

pub fn try_parse_timestamp(line: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(line.get(..30).unwrap_or(line)) {
        return Some(dt.with_timezone(&Utc));
    }

    let patterns: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.3f",
        "%Y-%m-%d %H:%M:%S",
        "%d/%b/%Y:%H:%M:%S",
    ];

    for pattern in patterns {
        let len = pattern.len() + 10;
        if let Some(slice) = line.get(..len.min(line.len())) {
            if let Ok(dt) = NaiveDateTime::parse_from_str(slice.trim(), pattern) {
                return Some(dt.and_utc());
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

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

pub fn detect_level(line: &str) -> LogLevel {
    let upper: String = line
        .chars()
        .take(256)
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if upper.contains("ALERT") || upper.contains("[ALERT]") {
        LogLevel::ALERT
    } else if upper.contains("ERROR") || upper.contains("[ERROR]") || upper.contains(" E ") {
        LogLevel::ERROR
    } else if upper.contains("WARN") || upper.contains("[WARN]") || upper.contains(" W ") {
        LogLevel::WARN
    } else if upper.contains("INFO") || upper.contains("[INFO]") || upper.contains(" I ") {
        LogLevel::INFO
    } else if upper.contains("DEBUG") || upper.contains("[DEBUG]") || upper.contains(" D ") {
        LogLevel::DEBUG
    } else if upper.contains("TRACE") || upper.contains("[TRACE]") {
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

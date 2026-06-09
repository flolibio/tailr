use chrono::{DateTime, Utc};
use tailr_protocol::{LogEntry, LogLevel};
use regex::Regex;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct LogFilter {
    pub levels: Vec<LogLevel>,
    pub time_from: Option<DateTime<Utc>>,
    pub time_to: Option<DateTime<Utc>>,
    pub pattern: Option<String>,
    pub compiled_regex: Option<Regex>,
}

impl LogFilter {
    pub fn new() -> Self {
        Self {
            levels: Vec::new(),
            time_from: None,
            time_to: None,
            pattern: None,
            compiled_regex: None,
        }
    }

    pub fn with_pattern(mut self, pattern: Option<String>) -> Self {
        self.compiled_regex = pattern.as_ref().and_then(|p| Regex::new(p).ok());
        self.pattern = pattern;
        self
    }

    pub fn with_levels(mut self, levels: Vec<LogLevel>) -> Self {
        self.levels = levels;
        self
    }

    pub fn with_time(mut self, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> Self {
        self.time_from = from;
        self.time_to = to;
        self
    }

    pub fn matches(&self, entry: &LogEntry) -> bool {
        if !self.levels.is_empty() && !self.levels.contains(&entry.level) {
            return false;
        }

        if let Some(ref timestamp) = entry.timestamp {
            if let Some(ref from) = self.time_from {
                if timestamp < from {
                    return false;
                }
            }
            if let Some(ref to) = self.time_to {
                if timestamp > to {
                    return false;
                }
            }
        } else {
            if self.time_from.is_some() || self.time_to.is_some() {
                return false;
            }
        }

        if let Some(ref re) = self.compiled_regex {
            if !re.is_match(&entry.raw) {
                return false;
            }
        } else if let Some(ref pattern) = self.pattern {
            if !entry.raw.contains(pattern) {
                return false;
            }
        }

        true
    }
}

impl Default for LogFilter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_filter(entries: &[LogEntry], filter: &LogFilter) -> Vec<LogEntry> {
    let before_count = entries.len();
    let result: Vec<LogEntry> = entries.iter().filter(|e| filter.matches(e)).cloned().collect();

    debug!(
        before = before_count,
        after = result.len(),
        level = ?filter.levels,
        time_from = ?filter.time_from,
        time_to = ?filter.time_to,
        pattern = ?filter.pattern,
        "Filter applied"
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_entry(level: LogLevel, raw: &str, ts: Option<DateTime<Utc>>) -> LogEntry {
        LogEntry {
            line_num: 1,
            raw: raw.to_string(),
            level,
            timestamp: ts,
            fields: None,
        }
    }

    #[test]
    fn test_filter_by_level() {
        let entries = vec![
            make_entry(LogLevel::ERROR, "error msg", None),
            make_entry(LogLevel::INFO, "info msg", None),
            make_entry(LogLevel::WARN, "warn msg", None),
            make_entry(LogLevel::ERROR, "another error", None),
        ];

        let filter = LogFilter {
            levels: vec![LogLevel::ERROR],
            ..Default::default()
        };

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].raw, "error msg");
        assert_eq!(result[1].raw, "another error");
    }

    #[test]
    fn test_filter_by_time_range() {
        let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let ts3 = Utc.with_ymd_and_hms(2024, 1, 1, 14, 0, 0).unwrap();

        let entries = vec![
            make_entry(LogLevel::INFO, "early", Some(ts1)),
            make_entry(LogLevel::INFO, "middle", Some(ts2)),
            make_entry(LogLevel::INFO, "late", Some(ts3)),
        ];

        let filter = LogFilter {
            time_from: Some(Utc.with_ymd_and_hms(2024, 1, 1, 11, 0, 0).unwrap()),
            time_to: Some(Utc.with_ymd_and_hms(2024, 1, 1, 13, 0, 0).unwrap()),
            ..Default::default()
        };

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].raw, "middle");
    }

    #[test]
    fn test_filter_by_pattern_regex() {
        let entries = vec![
            make_entry(LogLevel::INFO, "error: something failed", None),
            make_entry(LogLevel::INFO, "all good here", None),
            make_entry(LogLevel::INFO, "error: timeout occurred", None),
        ];

        let filter = LogFilter::new()
            .with_pattern(Some(r"error:\s+\w+".to_string()));

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_by_pattern_literal() {
        let entries = vec![
            make_entry(LogLevel::INFO, "connection refused", None),
            make_entry(LogLevel::INFO, "all good", None),
            make_entry(LogLevel::INFO, "connection timeout", None),
        ];

        let filter = LogFilter {
            pattern: Some("connection".to_string()),
            ..Default::default()
        };

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_combined() {
        let ts1 = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let ts2 = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();

        let entries = vec![
            make_entry(LogLevel::ERROR, "error at 10", Some(ts1)),
            make_entry(LogLevel::INFO, "info at 12", Some(ts2)),
            make_entry(LogLevel::ERROR, "error at 12", Some(ts2)),
        ];

        let filter = LogFilter {
            levels: vec![LogLevel::ERROR],
            time_from: Some(Utc.with_ymd_and_hms(2024, 1, 1, 11, 0, 0).unwrap()),
            ..Default::default()
        };

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].raw, "error at 12");
    }

    #[test]
    fn test_filter_no_timestamp_with_time_filter() {
        let entries = vec![
            make_entry(LogLevel::INFO, "no timestamp here", None),
        ];

        let filter = LogFilter {
            time_from: Some(Utc::now()),
            ..Default::default()
        };

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_filter_empty_entries() {
        let entries: Vec<LogEntry> = vec![];
        let filter = LogFilter::new();

        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_filter_no_filter() {
        let entries = vec![
            make_entry(LogLevel::INFO, "msg1", None),
            make_entry(LogLevel::ERROR, "msg2", None),
        ];

        let filter = LogFilter::new();
        let result = apply_filter(&entries, &filter);
        assert_eq!(result.len(), 2);
    }
}

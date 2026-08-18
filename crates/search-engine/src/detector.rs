//! 动态日志级别检测器（关键词匹配模式）
//!
//! 根据用户配置的关键词列表，按数组顺序逐个检测日志行的级别。
//! 使用零分配的 ASCII 大小写不敏感匹配。

use tailr_protocol::LogLevelConfig;

/// 编译后的级别（预处理关键词为小写，加速匹配）
struct CompiledLevel {
    name: String,
    keywords_lower: Vec<String>,
}

/// 动态级别检测器
pub struct LevelDetector {
    levels: Vec<CompiledLevel>,
    /// 配置指纹（名称+关键词的 hash，构建时算一次）。stats 缓存靠它
    /// 判断级别计数是否仍对当前配置有效——级别配置运行时热更后，
    /// 新检测器指纹不同，旧计数自动失效。
    fingerprint: u64,
}

impl LevelDetector {
    /// 从配置构建检测器
    pub fn from_config(config: &LogLevelConfig) -> Self {
        let levels = config
            .levels
            .iter()
            .map(|def| CompiledLevel {
                name: def.name.clone(),
                keywords_lower: def.keywords.iter().map(|k: &String| k.to_ascii_lowercase()).collect(),
            })
            .collect();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        for def in &config.levels {
            def.name.hash(&mut hasher);
            def.keywords.hash(&mut hasher);
        }
        Self {
            levels,
            fingerprint: hasher.finish(),
        }
    }

    /// 配置指纹：同配置稳定，配置变化（含热更后的新实例）即变化。
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// 返回所有已配置的级别名称（按配置顺序）。
    /// 用于判断 level 过滤集合是否覆盖全部已知级别。
    pub fn level_names(&self) -> Vec<&str> {
        self.levels.iter().map(|l| l.name.as_str()).collect()
    }

    /// 检测日志行的级别，返回级别名称（借用 self，零分配）。
    /// 无匹配返回 "UNKNOWN"。scanner 的统计热循环用这个变体。
    pub fn detect_ref(&self, line: &str) -> &str {
        let line_lower_bytes = line.as_bytes();
        let limit = line_lower_bytes.len().min(256);

        for compiled in &self.levels {
            for keyword in &compiled.keywords_lower {
                let keyword_bytes = keyword.as_bytes();
                if keyword_bytes.is_empty() {
                    continue;
                }
                if line_lower_bytes.len() < keyword_bytes.len() {
                    continue;
                }
                // 关键词长于扫描窗口（256B）时 limit - len 下溢：跳过该
                // 关键词防御，否则 debug panic / release 切片越界 panic。
                if keyword_bytes.len() > limit {
                    continue;
                }
                for i in 0..=limit - keyword_bytes.len() {
                    if line_lower_bytes[i..i + keyword_bytes.len()]
                        .iter()
                        .zip(keyword_bytes.iter())
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                    {
                        return compiled.name.as_str();
                    }
                }
            }
        }
        "UNKNOWN"
    }

    /// 检测日志行的级别，返回级别名称。
    /// 无匹配返回 "UNKNOWN"。
    pub fn detect(&self, line: &str) -> String {
        self.detect_ref(line).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tailr_protocol::{LevelDef, LogLevelConfig};

    fn make_config(preset: &str, levels: Vec<(&str, Vec<&str>)>) -> LogLevelConfig {
        LogLevelConfig {
            preset: preset.to_string(),
            levels: levels
                .into_iter()
                .map(|(name, keywords)| LevelDef {
                    name: name.to_string(),
                    keywords: keywords.into_iter().map(String::from).collect(),
                    color_light: "#000000".to_string(),
                    color_dark: "#FFFFFF".to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_keyword_longer_than_scan_window_does_not_panic() {
        // >256 字节的关键词 + 足够长的行：曾因 limit - len 下溢 panic。
        let long_keyword = "k".repeat(300);
        let line = format!("x{}y", long_keyword);
        let detector = crate::LevelDetector::from_config(&LogLevelConfig {
            preset: "custom".to_string(),
            levels: vec![LevelDef {
                name: "WEIRD".to_string(),
                keywords: vec![long_keyword],
                color_light: "#000000".to_string(),
                color_dark: "#ffffff".to_string(),
            }],
        });
        // 不 panic、返回 UNKNOWN（超窗关键词被安全跳过）即通过
        assert_eq!(detector.detect(&line), "UNKNOWN".to_string());
        assert_eq!(detector.detect_ref(&line), "UNKNOWN");
    }

    #[test]
    fn test_detect_basic() {
        let config = make_config("general", vec![
            ("ERROR", vec!["ERROR"]),
            ("WARN", vec!["WARN"]),
            ("INFO", vec!["INFO"]),
            ("DEBUG", vec!["DEBUG"]),
        ]);
        let detector = LevelDetector::from_config(&config);

        assert_eq!(detector.detect("2024-01-15 ERROR something failed"), "ERROR");
        assert_eq!(detector.detect("[WARN] disk almost full"), "WARN");
        assert_eq!(detector.detect("INFO: server started"), "INFO");
        assert_eq!(detector.detect("DEBUG: request received"), "DEBUG");
        assert_eq!(detector.detect("just some text"), "UNKNOWN");
    }

    #[test]
    fn test_detect_priority_order() {
        // ERROR 排在 INFO 前面，含 ERROR 的行应匹配 ERROR
        let config = make_config("test", vec![
            ("ERROR", vec!["ERROR"]),
            ("INFO", vec!["INFO"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("ERROR: level is INFO"), "ERROR");
    }

    #[test]
    fn test_detect_case_insensitive() {
        let config = make_config("test", vec![
            ("ERROR", vec!["error"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("2024-01-15 ERROR something"), "ERROR");
        assert_eq!(detector.detect("2024-01-15 error something"), "ERROR");
        assert_eq!(detector.detect("2024-01-15 Error something"), "ERROR");
    }

    #[test]
    fn test_detect_multiple_keywords() {
        let config = make_config("syslog", vec![
            ("ERR", vec!["ERR", "ERROR"]),
            ("WARNING", vec!["WARNING", "WARN"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("ERR: something"), "ERR");
        assert_eq!(detector.detect("ERROR: something"), "ERR");
        assert_eq!(detector.detect("WARNING: something"), "WARNING");
        assert_eq!(detector.detect("WARN: something"), "WARNING");
    }

    #[test]
    fn test_detect_unknown_fallback() {
        let config = make_config("general", vec![
            ("ERROR", vec!["ERROR"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("random text"), "UNKNOWN");
        assert_eq!(detector.detect(""), "UNKNOWN");
    }

    #[test]
    fn test_detect_empty_config() {
        let config = make_config("empty", vec![]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("ERROR: something"), "UNKNOWN");
    }

    #[test]
    fn test_detect_syslog_bracket_format() {
        let config = make_config("syslog", vec![
            ("EMERG", vec!["EMERG"]),
            ("ALERT", vec!["ALERT"]),
            ("CRIT", vec!["CRIT"]),
            ("ERR", vec!["ERR"]),
            ("WARNING", vec!["WARNING"]),
            ("NOTICE", vec!["NOTICE"]),
            ("INFO", vec!["INFO"]),
            ("DEBUG", vec!["DEBUG"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.detect("<0>EMERG: system is down"), "EMERG");
        assert_eq!(detector.detect("<1>ALERT: take action"), "ALERT");
        assert_eq!(detector.detect("<3>ERR: disk full"), "ERR");
    }

    #[test]
    fn test_level_names() {
        let config = make_config("general", vec![
            ("ERROR", vec!["ERROR"]),
            ("WARN", vec!["WARN"]),
            ("INFO", vec!["INFO"]),
        ]);
        let detector = LevelDetector::from_config(&config);
        assert_eq!(detector.level_names(), vec!["ERROR", "WARN", "INFO"]);
    }

    #[test]
    fn test_level_names_empty_config() {
        let config = make_config("empty", vec![]);
        let detector = LevelDetector::from_config(&config);
        assert!(detector.level_names().is_empty());
    }
}

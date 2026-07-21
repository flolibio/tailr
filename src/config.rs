//! Configuration file support for tailr.
//!
//! Loads config from TOML file with priority: CLI args > env vars > config file > defaults.
//! Auto-creates config directory and default config file on first run.

use std::fs;
use std::path::PathBuf;

use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use tailr_protocol::{LevelDef, LogLevelConfig};
// Re-use the LimitsConfig defined in tailr-server (which owns AppState and
// actually consumes the limits). Defining it there avoids a cyclic dep:
// tailr-server can't depend on the tailr binary crate.
pub use tailr_server::LimitsConfig;

/// Default config file template written on first run.
const DEFAULT_CONFIG_TEMPLATE: &str = r#"# tailr configuration file
# See: https://github.com/flolibio/tailr

# Log directories or files to serve (can specify multiple)
log = [
    "/var/log",
]

# Server bind address
bind = "0.0.0.0:7700"

# Security settings
# Token for authentication (empty = no auth required)
# When set, all requests must include Authorization: Bearer <token>
# You can set this via:
#   1. Config file (this file)
#   2. Settings dialog in the web UI
#   3. Environment variable TAILR_TOKEN
token = ""

# Timezone for log timestamps without explicit zone (e.g. "2026-07-05 22:43:21").
# Values: local (server timezone, default) | utc | +HH:MM | +HHMM
# Ctime lines with explicit offset (e.g. "Sun Jul 5 22:43:21 +08 2026") always use that offset.
log_timezone = "local"

# Daemon mode settings (optional)
# [daemon]
# Custom PID file path
# pid_file = "/run/tailr.pid"

# Custom log file path
# log_file = "/var/log/tailr.log"

# Resource limits (optional, all defaults shown)
# [limits]
# WebSocket 最大并发连接数（全局，所有客户端共享）
# 内网小团队（5-10 人，每人约 3-5 个 tab/连接）默认 50 够用；
# 单人使用可调到 20；大型团队可调到 100+
# max_ws_connections = 50
#
# REST API 限流：每 IP 每秒最大请求数（per-IP，每个客户端独立配额）
# 按 TCP 对端 IP 限流（tailr 直连部署，无反向代理）
# 单用户正常使用 < 5 req/s，留 4x 余量；内网多人各自独立桶互不影响
# rate_limit_rps = 20
"#;

/// Main configuration for tailr.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Log directories or files to serve.
    pub log: Vec<PathBuf>,
    /// Server bind address (e.g. "0.0.0.0:7700").
    pub bind: String,
    /// Daemon mode settings.
    pub daemon: DaemonConfig,
    /// Log level configuration (None = use default "general" preset).
    pub log_levels: Option<LogLevelConfig>,
    /// Token for authentication (empty = no auth required).
    pub token: String,
    /// Timezone for naive log timestamps. Values: "local" | "utc" | "+HH:MM".
    pub log_timezone: String,
    /// Resource limits for production hardening.
    pub limits: LimitsConfig,
}

/// Daemon-specific configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub pid_file: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log: vec![],
            bind: "0.0.0.0:7700".to_string(),
            daemon: DaemonConfig::default(),
            log_levels: Some(default_log_levels("general")),
            token: String::new(),
            log_timezone: "local".to_string(),
            limits: LimitsConfig::default(),
        }
    }
}

/// Returns the config directory path (`~/.config/tailr`).
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("tailr")
}

/// Returns the default config file path (`~/.config/tailr/config.toml`).
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Resolves the config file path from: CLI arg > TAILR_CONFIG env > default.
pub fn resolve_config_path(cli_config: Option<&PathBuf>) -> PathBuf {
    cli_config
        .cloned()
        .or_else(|| std::env::var("TAILR_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(config_path)
}

/// Ensures the config directory and default config file exist.
/// Creates them if missing. Does NOT overwrite an existing config file.
pub fn ensure_config_file(path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create config directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }

    if !path.exists() {
        write_default_config(path)?;
    }

    Ok(())
}

/// Writes the default config template to the specified path, overwriting if it exists.
pub fn write_default_config(path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create config directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }

    fs::write(path, DEFAULT_CONFIG_TEMPLATE).map_err(|e| {
        format!(
            "Failed to write config to {}: {}",
            path.display(),
            e
        )
    })
}

/// 返回指定预设的默认 LogLevelConfig。
pub fn default_log_levels(preset: &str) -> LogLevelConfig {
    let levels = match preset {
        "java" => vec![
            LevelDef { name: "FATAL".into(), keywords: vec!["FATAL".into()], color_light: "#CC2D26".into(), color_dark: "#FF6B63".into() },
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARN".into(), keywords: vec!["WARN".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
            LevelDef { name: "TRACE".into(), keywords: vec!["TRACE".into()], color_light: "#5F5E5A".into(), color_dark: "#B4B2A9".into() },
        ],
        "python" => vec![
            LevelDef { name: "CRITICAL".into(), keywords: vec!["CRITICAL".into()], color_light: "#CC2D26".into(), color_dark: "#FF6B63".into() },
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARNING".into(), keywords: vec!["WARNING".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
        ],
        "php" => vec![
            LevelDef { name: "ALERT".into(), keywords: vec!["ALERT".into()], color_light: "#CC2D26".into(), color_dark: "#FF6B63".into() },
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARNING".into(), keywords: vec!["WARNING".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "NOTICE".into(), keywords: vec!["NOTICE".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#5F5E5A".into(), color_dark: "#B4B2A9".into() },
        ],
        "go" => vec![
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARN".into(), keywords: vec!["WARN".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
        ],
        "rust" => vec![
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARN".into(), keywords: vec!["WARN".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
            LevelDef { name: "TRACE".into(), keywords: vec!["TRACE".into()], color_light: "#5F5E5A".into(), color_dark: "#B4B2A9".into() },
        ],
        "syslog" => vec![
            LevelDef { name: "EMERG".into(), keywords: vec!["EMERG".into()], color_light: "#CC2D26".into(), color_dark: "#FF6B63".into() },
            LevelDef { name: "ALERT".into(), keywords: vec!["ALERT".into()], color_light: "#D4421E".into(), color_dark: "#FF8A65".into() },
            LevelDef { name: "CRIT".into(), keywords: vec!["CRIT".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "ERR".into(), keywords: vec!["ERR".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "WARNING".into(), keywords: vec!["WARNING".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "NOTICE".into(), keywords: vec!["NOTICE".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#5F5E5A".into(), color_dark: "#B4B2A9".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#5F5E5A".into(), color_dark: "#B4B2A9".into() },
        ],
        _ => vec![
            // "general" 或未知预设
            LevelDef { name: "ERROR".into(), keywords: vec!["ERROR".into()], color_light: "#A32D2D".into(), color_dark: "#F09595".into() },
            LevelDef { name: "WARN".into(), keywords: vec!["WARN".into()], color_light: "#854F0B".into(), color_dark: "#EF9F27".into() },
            LevelDef { name: "INFO".into(), keywords: vec!["INFO".into()], color_light: "#0C447C".into(), color_dark: "#85B7EB".into() },
            LevelDef { name: "DEBUG".into(), keywords: vec!["DEBUG".into()], color_light: "#3B6D11".into(), color_dark: "#97C459".into() },
        ],
    };

    LogLevelConfig {
        preset: preset.to_string(),
        levels,
    }
}

/// Loads the full configuration by merging providers in priority order:
///
/// 1. Defaults (lowest)
/// 2. TOML config file
/// 3. Environment variables (`TAILR_*`)
/// 4. CLI args (highest)
///
/// CLI args are provided as optional overrides — `None` means "use lower priority".
pub fn load_config(
    config_path: &PathBuf,
    cli_log: Option<Vec<PathBuf>>,
    cli_bind: Option<&str>,
    cli_daemon: bool,
    cli_pid_file: Option<&PathBuf>,
    cli_log_file: Option<&PathBuf>,
) -> Result<Config, String> {
    let cli_overrides = CliOverrides {
        log: cli_log,
        bind: cli_bind.map(String::from),
        daemon: if cli_daemon || cli_pid_file.is_some() || cli_log_file.is_some() {
            Some(DaemonOverrides {
                pid_file: cli_pid_file.cloned(),
                log_file: cli_log_file.cloned(),
            })
        } else {
            None
        },
    };

    let mut figment = Figment::new()
        .merge(Serialized::defaults(Config::default()))
        .merge(Toml::file(config_path));

    let env_log = std::env::var("TAILR_LOG_DIR").ok();
    let env_bind = std::env::var("TAILR_BIND").ok();
    let env_token = std::env::var("TAILR_TOKEN").ok();

    if env_log.is_some() || env_bind.is_some() || env_token.is_some() {
        let env_overrides = EnvOverrides {
            log: env_log.map(|v| {
                v.split(',')
                    .map(|s| PathBuf::from(s.trim()))
                    .collect()
            }),
            bind: env_bind,
            token: env_token,
        };
        figment = figment.merge(Serialized::defaults(env_overrides));
    }

    figment = figment.merge(Serialized::defaults(cli_overrides));

    figment.extract().map_err(|e| format!("Failed to load config: {}", e))
}

/// Helper for building config from resolved components.
/// Determines final log paths considering all sources.
pub fn resolve_log_paths(config: &Config) -> Vec<PathBuf> {
    if !config.log.is_empty() {
        return config.log.clone();
    }

        let default = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("logs");
    vec![default]
}

#[derive(Serialize)]
struct CliOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<Vec<PathBuf>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    daemon: Option<DaemonOverrides>,
}

#[derive(Serialize)]
struct DaemonOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pid_file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_file: Option<PathBuf>,
}

#[derive(Serialize)]
struct EnvOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<Vec<PathBuf>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.log.is_empty());
        assert_eq!(config.bind, "0.0.0.0:7700");
        assert!(config.daemon.pid_file.is_none());
        assert!(config.daemon.log_file.is_none());
        assert!(config.token.is_empty());
        // Limits defaults
        assert_eq!(config.limits.max_ws_connections, 50);
        assert_eq!(config.limits.rate_limit_rps, 20);
    }

    #[test]
    fn test_limits_default_independent() {
        let limits = LimitsConfig::default();
        assert_eq!(limits.max_ws_connections, 50);
        assert_eq!(limits.rate_limit_rps, 20);
    }

    #[test]
    fn test_limits_backward_compat_no_section() {
        // Old config without [limits] section should still load (uses defaults).
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut f = fs::File::create(&config_path).unwrap();
        writeln!(f, "bind = \"127.0.0.1:8080\"").unwrap();

        let config = load_config(&config_path, None, None, false, None, None).unwrap();
        assert_eq!(config.limits.max_ws_connections, 50);
        assert_eq!(config.limits.rate_limit_rps, 20);
    }

    #[test]
    fn test_limits_custom_values() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut f = fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"
[limits]
max_ws_connections = 100
rate_limit_rps = 50
"#
        )
        .unwrap();

        let config = load_config(&config_path, None, None, false, None, None).unwrap();
        assert_eq!(config.limits.max_ws_connections, 100);
        assert_eq!(config.limits.rate_limit_rps, 50);
    }

    #[test]
    fn test_limits_partial_section_uses_defaults() {
        // Partial [limits] section: missing keys fall back to defaults via #[serde(default)].
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut f = fs::File::create(&config_path).unwrap();
        write!(f, "[limits]\nmax_ws_connections = 10\n").unwrap();

        let config = load_config(&config_path, None, None, false, None, None).unwrap();
        assert_eq!(config.limits.max_ws_connections, 10);
        assert_eq!(config.limits.rate_limit_rps, 20); // default
    }

    #[test]
    fn test_load_config_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut f = fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"
log = ["/var/log/app", "/var/log/nginx"]
bind = "127.0.0.1:8080"

[daemon]
pid_file = "/run/tailr.pid"
"#
        )
        .unwrap();

        let config = load_config(&config_path, None, None, false, None, None).unwrap();
        assert_eq!(config.log, vec![PathBuf::from("/var/log/app"), PathBuf::from("/var/log/nginx")]);
        assert_eq!(config.bind, "127.0.0.1:8080");
        assert_eq!(config.daemon.pid_file, Some(PathBuf::from("/run/tailr.pid")));
        assert!(config.daemon.log_file.is_none());
    }

    #[test]
    fn test_cli_overrides_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut f = fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"
bind = "127.0.0.1:8080"
"#
        )
        .unwrap();

        let cli_log = vec![PathBuf::from("/tmp")];
        let config = load_config(&config_path, Some(cli_log), Some("0.0.0.0:9999"), false, None, None).unwrap();
        assert_eq!(config.log, vec![PathBuf::from("/tmp")]);
        assert_eq!(config.bind, "0.0.0.0:9999");
    }

    #[test]
    fn test_missing_config_file_uses_defaults() {
        let config_path = PathBuf::from("/tmp/nonexistent_tailr_config_test.toml");
        let config = load_config(&config_path, None, None, false, None, None).unwrap();
        assert!(config.log.is_empty());
        assert_eq!(config.bind, "0.0.0.0:7700");
    }

    #[test]
    fn test_ensure_config_file_creates_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subdir").join("config.toml");

        ensure_config_file(&path).unwrap();
        assert!(path.exists());

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("tailr configuration file"));
        assert!(contents.contains("bind"));
    }

    #[test]
    fn test_ensure_config_file_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "bind = \"custom\"").unwrap();

        ensure_config_file(&path).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "bind = \"custom\"");
    }
}

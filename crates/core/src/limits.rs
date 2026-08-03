//! Resource limits configuration.
//!
//! All thresholds are user-tunable via the `[limits]` section in config.toml.
//! Owned by the core layer (part of the config schema); consumed by the Web
//! layer (rate limiter / WS cap / worker count).

use serde::{Deserialize, Serialize};

/// Resource limits for production hardening.
/// All thresholds are user-tunable via `[limits]` section in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// Global cap on concurrent WebSocket connections (across all clients).
    /// Default 50 covers a small LAN team (5-10 users × 3-5 tabs each) with
    /// headroom. Hitting the cap usually signals a frontend bug (WS not
    /// released) or abnormal load.
    pub max_ws_connections: usize,
    /// Max REST requests per second per client IP (GCRA sustained rate).
    /// Default 20 — single-user normal usage is < 5 req/s.
    /// Actual burst is covered by `burst_size = rate_limit_rps * 3`
    /// (internal-derived, not exposed to the user).
    pub rate_limit_rps: u32,
    /// Enable gzip response compression. Default false.
    /// On gigabit LAN, miniz_oxide throughput (~70 MB/s) is below network
    /// bandwidth (125 MB/s), so the CPU cost (~14ms/MB) exceeds transfer
    /// savings — measured 10-15% slower on 1MB responses.
    /// On public/weak network/VPN, bandwidth is typically < 70 MB/s (560 Mbps)
    /// and compression pays off (1MB response: 5x faster on home broadband,
    /// 20x on 4G, 29x on weak WiFi). User decides based on deployment.
    pub enable_compression: bool,
    /// Tokio async worker thread count. Default 2.
    /// tailr is IO-bound (log tailing + WebSocket fan-out); 2 workers cover
    /// single-user and small-team scenarios. Raise to 4+ for large teams or
    /// heavy concurrent file-open operations. Lower to 1 for memory-constrained
    /// containers. (The blocking pool for mmap/HTTP is capped at 4 internally.)
    pub workers: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_ws_connections: 50,
            rate_limit_rps: 20,
            enable_compression: false,
            workers: 2,
        }
    }
}

impl LimitsConfig {
    /// Validate at config-load time so misconfigs produce a clean stderr message
    /// and graceful exit — not a panic deep in `app()`.
    pub fn validate(&self) -> Result<(), String> {
        if self.rate_limit_rps == 0 {
            return Err(
                "rate_limit_rps must be > 0 (set to a higher value to loosen, \
                 not 0 to disable)"
                    .to_string(),
            );
        }
        if self.max_ws_connections == 0 {
            return Err(
                "max_ws_connections must be > 0 (would reject every WS connection)"
                    .to_string(),
            );
        }
        if self.workers == 0 {
            return Err("workers must be > 0 (tokio needs at least 1 worker thread)".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_config_default() {
        let l = LimitsConfig::default();
        assert_eq!(l.max_ws_connections, 50);
        assert_eq!(l.rate_limit_rps, 20);
        assert_eq!(l.workers, 2);
    }

    #[test]
    fn test_limits_validate_rejects_zero_rps() {
        let l = LimitsConfig {
            rate_limit_rps: 0,
            ..Default::default()
        };
        assert!(l.validate().is_err());
    }

    #[test]
    fn test_limits_validate_rejects_zero_ws_connections() {
        let l = LimitsConfig {
            max_ws_connections: 0,
            ..Default::default()
        };
        assert!(l.validate().is_err());
    }

    #[test]
    fn test_limits_validate_rejects_zero_workers() {
        let l = LimitsConfig {
            workers: 0,
            ..Default::default()
        };
        assert!(l.validate().is_err());
    }

    #[test]
    fn test_limits_validate_accepts_defaults() {
        assert!(LimitsConfig::default().validate().is_ok());
    }
}

# Config File Feature Planning

## Overview

Add configuration file support to tailr, allowing users to preset defaults instead of passing CLI arguments every time.

## Current State

```bash
# Current: must pass all args every time
tailr --log /var/log/app /var/log/nginx --bind 0.0.0.0:7700 --daemon

# Environment variables (limited)
TAILR_LOG_DIR=/var/log/app tailr
```

**Priority**: CLI args > `TAILR_LOG_DIR` env > `<exe_dir>/logs`

## Proposed Solution

### Config File Location

| Platform | Path | Status |
|----------|------|--------|
| Linux | `~/.config/tailr/config.toml` | Current |
| macOS | `~/.config/tailr/config.toml` | Current |
| Windows | `{FOLDERID_RoamingAppData}\tailr\config.toml` | Future |

**Current implementation**: Fixed path `~/.config/tailr/config.toml` (Linux/macOS)
**Future**: Add Windows support using `directories` crate when building Windows package

Override with:
- `--config <path>` CLI flag
- `TAILR_CONFIG` environment variable

**Auto-initialization**: Create config directory and default config file on first run if not exists.

### Default Config File

When `~/.config/tailr/config.toml` does not exist, create it with:

```toml
# tailr configuration file
# See: https://github.com/flolibio/tailr

# Log directories or files to serve (can specify multiple)
# Default: current directory's "logs" subdirectory
# log = [
#     "/var/log",
#     "/var/log/app"
# ]

# Server bind address
bind = "0.0.0.0:7700"

# Daemon mode settings (optional)
[daemon]
# Custom PID file path
# pid_file = "/run/tailr.pid"

# Custom log file path
# log_file = "/var/log/tailr.log"
```

### First Run Behavior

```bash
# First run: creates ~/.config/tailr/ and default config.toml
tailr --show-config

# Output:
# Config file: ~/.config/tailr/config.toml
# Config file created with default values.
```

**Behavior**:
1. Check if `~/.config/tailr/` exists
2. If not, create directory
3. If `config.toml` not exists, create with default template
4. Load config and start server

### Config File Format (TOML)

```toml
# ~/.config/tailr/config.toml

# Log directories or files to serve
log = [
    "/var/log/app",
    "/var/log/nginx",
    "/var/log/syslog"
]

# Bind address
bind = "0.0.0.0:7700"

# Daemon mode settings
[daemon]
pid_file = "/run/tailr.pid"
log_file = "/var/log/tailr.log"
```

### Priority Order

```
CLI args (highest) > Env vars > Config file > Defaults (lowest)
```

Example:
```bash
# Config file has: bind = "0.0.0.0:7700"
# This overrides to 8080:
tailr --bind 0.0.0.0:8080
```

## Implementation Plan

### Dependencies

```toml
[dependencies]
figment = { version = "0.10", features = ["toml", "env"] }
# directories = "5"  # Future: Windows support
```

### Config Struct

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub log: Vec<PathBuf>,
    pub bind: String,
    pub daemon: DaemonConfig,
}

#[derive(Debug, Deserialize)]
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
        }
    }
}
```

### CLI Args (All Optional)

```rust
#[derive(Parser, Serialize)]
struct Cli {
    /// Custom config file path
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<PathBuf>,

    /// Log directories or files
    #[arg(short, long, num_args = 1..)]
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<Vec<PathBuf>>,

    /// Bind address
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    bind: Option<String>,

    // ... other args
}
```

### Config Loading

```rust
use figment::{Figment, providers::{Serialized, Toml, Env, Format}};

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("tailr")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn load_config(cli: &Cli) -> Config {
    // 1. Find config file
    let config_file = cli.config.clone()
        .or_else(|| std::env::var("TAILR_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(config_path);

    // 2. Merge in priority order
    Figment::new()
        .merge(Serialized::defaults(Config::default()))
        .merge(Toml::file(&config_file))
        .merge(Env::prefixed("TAILR_"))
        .merge(Serialized::defaults(cli))
        .extract()
        .expect("Failed to load config")
}
```

### New CLI Commands

```bash
# Generate example config file
tailr --generate-config > ~/.config/tailr/config.toml

# Show config file location
tailr --show-config

# Use custom config
tailr --config /path/to/config.toml
```

## File Changes

| File | Change |
|------|--------|
| `Cargo.toml` | Add `figment` dependency |
| `src/main.rs` | Refactor CLI args to `Option<T>`, add config loading |
| `src/config.rs` | **New** - Config struct and loading logic |
| `src/daemon.rs` | Use config for default paths |

### Future Windows Support

When adding Windows support:
1. Add `directories = "5"` to dependencies
2. Update `config_path()` to use `ProjectDirs::from("", "", "tailr")`
3. Test on Windows with `%APPDATA%\tailr\config.toml`

## Testing

```bash
# Test default config location
tailr --show-config

# Test custom config
tailr --config ./test.toml --log /tmp

# Test env override
TAILR_LOG_DIR=/tmp tailr --show-config

# Test CLI override
tailr --config ./test.toml --bind 0.0.0.0:9090

# Test auto-initialization
rm -rf ~/.config/tailr
tailr --show-config  # Should create directory and show path
```

## Example Config File

```toml
# tailr configuration file
# See: https://github.com/flolibio/tailr

# Log directories or files to serve (can specify multiple)
log = [
    "/var/log/app",
    "/var/log/nginx"
]

# Server bind address
bind = "0.0.0.0:7700"

# Daemon mode settings (optional)
[daemon]
# Custom PID file path
# pid_file = "/run/tailr.pid"

# Custom log file path
# log_file = "/var/log/tailr.log"
```

## References

- [figment crate](https://docs.rs/figment)
- [directories crate](https://docs.rs/directories) (future Windows support)
- [Starship config](https://starship.rs/config/)
- [Ripgrep config](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md#configuration-file)

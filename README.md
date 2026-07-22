<p align="center">
  <img src="frontend/public/logo-192x192.png" alt="tailr" width="120">
</p>

<h1 align="center">tailr</h1>

<p align="center">
  A blazing-fast log tail & search server. Single binary, web UI, real-time streaming.
</p>

<p align="center">
  <a href="https://github.com/flolibio/tailr/releases"><img src="https://img.shields.io/github/v/release/flolibio/tailr" alt="Release"></a>
  <a href="https://github.com/flolibio/tailr/stargazers"><img src="https://img.shields.io/github/stars/flolibio/tailr" alt="Stars"></a>
  <a href="https://github.com/flolibio/tailr/blob/main/LICENSE"><img src="https://img.shields.io/github/license/flolibio/tailr" alt="License"></a>
  <a href="https://github.com/flolibio/tailr/actions"><img src="https://img.shields.io/github/actions/workflow/status/flolibio/tailr/ci.yml" alt="Build"></a>
</p>

<p align="center">
  <a href="#quickstart">Quick Start</a> •
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#api">API</a> •
  <a href="#license">License</a>
</p>

---

## Demo

🌐 **Live Demo:** [tailr.flolib.com](https://tailr.flolib.com/)

**Real-time log tailing with multi-keyword filtering:**
- 📡 WebSocket-based live streaming
- 🔍 Regex search with mmap
- 🎨 Configurable log levels with color coding
- 🔒 Optional token authentication

## Why tailr?

| Feature | tailr | kail | goaccess | lnav |
|---------|-------|------|----------|------|
| **Single binary** | ✅ | ❌ | ❌ | ❌ |
| **Web UI** | ✅ | ✅ | ✅ | ❌ |
| **Real-time tail** | ✅ | ✅ | ❌ | ✅ |
| **Multi-file tabs** | ✅ | ❌ | ❌ | ❌ |
| **Regex search** | ✅ | ❌ | ❌ | ✅ |
| **Log level detection** | ✅ | ❌ | ✅ | ✅ |
| **Memory-mapped** | ✅ | ❌ | ❌ | ❌ |
| **Self-upgrade (Web + CLI)** | ✅ | ❌ | ❌ | ❌ |
| **Token auth** | ✅ | ❌ | ❌ | ❌ |
| **Config presets** | ✅ | ❌ | ❌ | ❌ |

## Features

- **Real-time tail** — WebSocket-based live log streaming
- **Multi-keyword filter** — AND logic, like `grep kw1 | grep kw2`
- **Fast search** — mmap-based grep with regex support
- **Multi-file tabs** — Open multiple log files side by side, each with independent filter state
- **Bookmarks** — Mark lines for quick jump-back, persisted per file
- **Share links** — Generate a URL encoding the current file + filters; opening it restores the exact view
- **Configurable log levels** — User-defined levels, keywords, and colors with 7 presets (General, Java, Python, PHP, Go, Rust, syslog)
- **Single binary** — No dependencies, no runtime, just run
- **Web UI** — Built-in Vue 3 SPA, no separate frontend deployment
- **Log rotation aware** — Detects inode changes, handles logrotate
- **Self-upgrade** — One-click update from the Web UI or `tailr upgrade` CLI; auto-restarts after replacing the binary
- **Update notifications** — Background check for new releases; badge + toast in the Web UI when an update is available
- **Token authentication** — Optional Bearer token for secure access
- **Path validation** — Prevents directory traversal attacks
- **Resource limits** — Configurable WebSocket connection cap and per-IP REST rate limiting for production hardening
- **Optional gzip compression** — Opt-in response compression for public/weak-network access (off by default; gigabit LAN is faster without it)
- **Multi-language UI** — English (default) and Chinese, with easy extensibility
- **Cross-platform** — Linux (x86_64/ARM64), macOS

## Quickstart

```bash
# Run with specific log directories
tailr --log /var/log/app /var/log/nginx

# Run with a single file
tailr --log /var/log/syslog

# Custom bind address
tailr --log /var/log -b 127.0.0.1:8080

# With authentication
TAILR_TOKEN=your-secret tailr --log /var/log/app
```

Open `http://localhost:7700` in your browser.

## Installation

### Download binary

Download the latest binary from [GitHub Releases](https://github.com/flolibio/tailr/releases).

```bash
# Linux x86_64
curl -LO https://github.com/flolibio/tailr/releases/latest/download/tailr-x86_64-linux-musl.tar.gz
tar xzf tailr-x86_64-linux-musl.tar.gz
sudo mv tailr /usr/local/bin/

# Linux ARM64
curl -LO https://github.com/flolibio/tailr/releases/latest/download/tailr-aarch64-linux-musl.tar.gz
tar xzf tailr-aarch64-linux-musl.tar.gz
sudo mv tailr /usr/local/bin/
```

### Build from source

```bash
# Clone
git clone https://github.com/flolibio/tailr.git
cd tailr

# Build frontend + Rust binary
make build

# Or just run in dev mode
make dev
```

### Cross-compile for Linux (from macOS)

```bash
make build-linux       # x86_64
make build-linux-arm   # aarch64
make release           # both + frontend
```

Uses Docker with musl for static binaries (no glibc dependency).

## CLI

```
tailr [OPTIONS]           # Start server (default)
tailr <COMMAND>           # Run a subcommand

Commands:
  init                    Initialize config file
  config                  Print config file contents
  stop                    Stop running daemon
  restart                 Restart running daemon (stops + re-execs with the same args)
  status                  Show daemon status
  systemd                 Generate systemd service file
  launchd                 Generate launchd plist file (macOS)
  upgrade                 Check for updates and upgrade tailr to the latest version

Options:
  -l, --log <LOG>...         Log directories or files to serve (can specify multiple)
  -b, --bind <BIND>          Bind address [default: 0.0.0.0:7700]
  -d, --daemon               Run as daemon in background
      --config <CONFIG>      Custom config file path
      --pid-file <PID_FILE>  Custom PID file path
      --log-file <LOG_FILE>  Custom log file path for daemon mode
  -h, --help                 Print help
  -V, --version              Print version
```

**Priority:** CLI args > Config file > `TAILR_*` env vars > Defaults

### Config File

```bash
# Initialize config file
tailr init

# Print config file contents
tailr config

# Use custom config file
tailr --config /path/to/config.toml
```

The config file is located at `~/.tailr/config.toml` by default. All tailr files (config, PID, logs, restart state) live in `~/.tailr/`.

```toml
# Log directories or files to serve
log = ["/var/log"]

# Server bind address
bind = "0.0.0.0:7700"

# Token for authentication (empty = no auth required)
token = ""

# Resource limits (optional, all defaults shown)
# [limits]
# max_ws_connections = 50       # global WebSocket connection cap
# rate_limit_rps = 20           # per-client-IP REST requests/second (burst = ×3)
# enable_compression = false    # gzip; off by default (LAN is faster without it)

# Log level configuration (optional, uses "general" preset by default)
[log_levels]
preset = "python"

[[log_levels.levels]]
name = "CRITICAL"
keywords = ["CRITICAL"]
colorLight = "#CC2D26"
colorDark = "#FF6B63"

[[log_levels.levels]]
name = "ERROR"
keywords = ["ERROR"]
colorLight = "#A32D2D"
colorDark = "#F09595"

[[log_levels.levels]]
name = "WARNING"
keywords = ["WARNING"]
colorLight = "#854F0B"
colorDark = "#EF9F27"

[[log_levels.levels]]
name = "INFO"
keywords = ["INFO"]
colorLight = "#0C447C"
colorDark = "#85B7EB"

[[log_levels.levels]]
name = "DEBUG"
keywords = ["DEBUG"]
colorLight = "#3B6D11"
colorDark = "#97C459"
```

**Available presets:** general, java, python, php, go, rust, syslog

Log levels can also be configured via the Web UI under Settings → Log Levels.

### Daemon Mode

Run tailr as a background daemon instead of using `nohup`:

```bash
# Start in daemon mode
tailr -d -l /var/log/app /var/log/nginx

# Check status
tailr status

# Stop daemon
tailr stop
```

**PID/Log files** are stored in `~/.tailr/` by default. Customize with:

```bash
tailr -d -l /var/log/app \
  --pid-file /run/tailr.pid \
  --log-file /var/log/tailr.log
```

### System Service

#### systemd (Linux)

```bash
# Generate and install service file
tailr systemd -l /var/log/app | sudo tee /etc/systemd/system/tailr.service

# Enable and start
sudo systemctl enable --now tailr

# Check status
sudo systemctl status tailr
```

#### launchd (macOS)

```bash
# Generate and install plist
tailr launchd -l /var/log/app > ~/Library/LaunchAgents/com.tailr.plist

# Load and start
launchctl load ~/Library/LaunchAgents/com.tailr.plist

# Check status
launchctl list | grep tailr
```

### Self-Upgrade

```bash
# Check for updates
tailr upgrade --check

# Upgrade to latest version
tailr upgrade

# Restart the daemon to apply an upgrade
tailr restart
```

**From the Web UI:** Settings → About → "Check for updates". If a newer version is found, click "Upgrade" — tailr downloads the new binary, replaces itself atomically, and restarts automatically. The page polls `/api/health` and reloads once the server is back.

**Note:** Automatic upgrade is supported on Linux x86_64/ARM64 only. On macOS, the Web UI shows the new version and a download link. The upgrade endpoint requires a token to be set (replacing the binary is an RCE-class operation); `tailr upgrade` from the CLI works without a token.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TAILR_LOG_DIR` | `<exe_dir>/logs` | Comma-separated log directories |
| `TAILR_BIND` | `0.0.0.0:7700` | Listen address |
| `TAILR_CONFIG` | `~/.tailr/config.toml` | Config file path |
| `TAILR_TOKEN` | — | Authentication token (overrides config file) |
| `RUST_LOG` | — | Tracing filter (e.g. `tailr=debug`) |

## API

| Route | Method | Description |
|-------|--------|-------------|
| `/api/files` | GET | List log files (filtered: text files only) |
| `/api/file/tail` | GET | Last N lines (`?path=&lines=`) |
| `/api/config/log-levels` | GET | Get current log level configuration |
| `/api/config/log-levels` | POST | Save log level configuration (hot-reload + persist to config.toml) |
| `/api/upgrade/check` | GET | Check for a newer release (`?force=true` bypasses cache) |
| `/api/upgrade` | POST | Download + replace binary + restart (requires token + CSRF header) |
| `/api/health` | GET | Status + uptime + version |
| `/ws` | WS | Real-time log streaming |

### WebSocket Protocol

```json
// Subscribe to a file
{"type": "subscribe", "path": "/var/log/app.log"}

// Receive new entries
{"type": "append", "path": "/var/log/app.log", "seq": 42, "entries": [...]}

// Catchup on reconnect
{"type": "catchup", "path": "/var/log/app.log", "entries": [...], "lastSeq": 100}

// Server-pushed update notification (broadcast to all clients)
{"type": "updateAvailable", "latestVersion": "0.9.5", "currentVersion": "0.9.4", "releaseUrl": "..."}
```

## Development

```bash
# Terminal 1: Rust backend
cargo run

# Terminal 2: Vite dev server (with proxy)
cd frontend && npm run dev
```

Vite proxies `/api` and `/ws` to `http://localhost:7700`.

### Testing

```bash
make test              # Run all checks (clippy + vue-tsc)
make test-backend      # cargo test + cargo clippy
make test-frontend     # vue-tsc --noEmit
```

### Internationalization (i18n)

The web UI supports multiple languages:
- **English (en-US)** — Default
- **Chinese (zh-CN)** — 简体中文

**Adding a new language:**
1. Create a new locale file in `frontend/src/locales/` (e.g., `ja-JP.json`)
2. Copy the structure from `en-US.json` and translate all strings
3. Update `frontend/src/locales/index.ts` to include the new locale in the type definition
4. Add the locale option to the language switcher in `SettingsDialog.vue`
5. Run `cd frontend && npm run check:i18n` to verify key completeness across locales

The language preference is persisted in localStorage and auto-detected from the browser on first visit.

## Architecture

```
src/main.rs           # CLI (clap), env vars, starts axum server
src/config.rs         # Config loading (figment), presets, persistence
crates/
  protocol/           # Shared types: LogEntry, WSMessage, LevelDef, LogLevelConfig
  tail-engine/        # File watching (notify), LineIndex (mmap), TailSession
  search-engine/      # Grep-based search, LevelDetector (dynamic log levels)
  server/             # Axum app: REST API, WebSocket, static files
frontend/             # Vue 3 + TypeScript + Vite SPA
  composables/        # useLogLevels (presets, colors, dynamic CSS)
  components/         # Settings UI (LogLevelSettings, ColorPicker)
```

## Security

- **Token Authentication** — Optional Bearer token via config, environment variable, or Web UI
- **Path Validation** — All file endpoints validated against configured directories
- **CSRF Protection** — Restricted CORS headers + X-Requested-With check
- **Error Sanitization** — Generic error messages to client, detailed logs server-side

## License

[MIT](LICENSE)

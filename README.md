<p align="center">
  <img src="frontend/public/logo-192x192.png" alt="tailr" width="120">
</p>

<h1 align="center">tailr</h1>

<p align="center">
  A blazing-fast log tail & search server. Single binary, web UI, real-time streaming.
</p>

<p align="center">
  <a href="#quickstart">Quick Start</a> •
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#api">API</a> •
  <a href="#license">License</a>
</p>

---

## Features

- **Real-time tail** — WebSocket-based live log streaming
- **Multi-keyword filter** — AND logic, like `grep kw1 | grep kw2`
- **Fast search** — mmap-based grep with regex support
- **Log level detection** — Auto-detects ALERT/ERROR/WARN/INFO/DEBUG/TRACE
- **Single binary** — No dependencies, no runtime, just run
- **Web UI** — Built-in Vue 3 SPA, no separate frontend deployment
- **Log rotation aware** — Detects inode changes, handles logrotate
- **Self-upgrade** — One command to update to the latest version
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
```

Open `http://localhost:7700` in your browser.

## Installation

### Download binary

Download the latest binary from [GitHub Releases](https://github.com/wunamesst/tailr/releases).

### Build from source

```bash
# Clone
git clone https://github.com/wunamesst/tailr.git
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
tailr upgrade [--check]   # Check/perform self-upgrade

Options:
  -l, --log <LOG>...  Log directories or files to serve (can specify multiple)
      -b, --bind <BIND>   Bind address [default: 0.0.0.0:7700]
  -h, --help          Print help
  -V, --version       Print version

Subcommands:
  upgrade             Check for updates and upgrade tailr to the latest version
    -c, --check       Only check for updates without installing
```

**Priority:** CLI args > `TAILR_LOG_DIR` env var > `<exe_dir>/logs`

### Self-Upgrade

```bash
# Check for updates
tailr upgrade --check

# Upgrade to latest version
tailr upgrade
```

**Note:** The upgrade replaces the binary atomically. If tailr is running as a service, you'll need to restart it after upgrading.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TAILR_LOG_DIR` | `<exe_dir>/logs` | Comma-separated log directories (fallback if no CLI args) |
| `TAILR_BIND` | `0.0.0.0:7700` | Listen address |
| `RUST_LOG` | — | Tracing filter (e.g. `tailr=debug`) |

## API

| Route | Method | Description |
|-------|--------|-------------|
| `/api/files` | GET | List log files (filtered: text files only) |
| `/api/file/content` | GET | Paginated file content (`?path=&offset=&limit=`) |
| `/api/file/tail` | GET | Last N lines (`?path=&lines=`) |
| `/api/file/info` | GET | File metadata + line count |
| `/api/search` | GET | Grep search (`?path=&q=&regex=&levels=&context=&limit=`) |
| `/api/health` | GET | Status + uptime |
| `/ws` | WS | Real-time log streaming |

### WebSocket Protocol

```json
// Subscribe to a file
{"type": "subscribe", "path": "/var/log/app.log"}

// Receive new entries
{"type": "append", "path": "/var/log/app.log", "seq": 42, "entries": [...]}

// Catchup on reconnect
{"type": "catchup", "path": "/var/log/app.log", "entries": [...], "lastSeq": 100}
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
4. Add the locale option to the language switcher in `SettingsPanel.vue`

The language preference is persisted in localStorage and auto-detected from the browser on first visit.

## Architecture

```
src/main.rs           # CLI (clap), env vars, starts axum server
crates/
  protocol/           # Shared types: LogEntry, WSMessage, LogLevel
  tail-engine/        # File watching (notify), LineIndex (mmap)
  search-engine/      # Grep-based search (grep-regex/grep-searcher)
  server/             # Axum app: REST API, WebSocket, static files
frontend/             # Vue 3 + TypeScript + Vite SPA
```

## License

[MIT](LICENSE)

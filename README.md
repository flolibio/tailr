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

Open `http://localhost:3000` in your browser.

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
tailr [OPTIONS]

Options:
  -l, --log <LOG>...  Log directories or files to serve (can specify multiple)
  -b, --bind <BIND>   Bind address [default: 0.0.0.0:3000]
  -h, --help          Print help
  -V, --version       Print version
```

**Priority:** CLI args > `TAILR_LOG_DIR` env var > `<exe_dir>/logs`

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TAILR_LOG_DIR` | `<exe_dir>/logs` | Comma-separated log directories (fallback if no CLI args) |
| `TAILR_BIND` | `0.0.0.0:3000` | Listen address |
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

Vite proxies `/api` and `/ws` to `http://localhost:3000`.

### Testing

```bash
make test              # Run all checks (clippy + vue-tsc)
make test-backend      # cargo test + cargo clippy
make test-frontend     # vue-tsc --noEmit
```

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

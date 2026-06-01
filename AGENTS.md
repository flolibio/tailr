# AGENTS.md

## What this is

tailr: a single-binary log tail/search server. Rust backend (axum) + Vue 3 frontend. Serves a web UI that tails and searches log files via WebSocket and REST.

## Architecture

```
src/main.rs           # Binary entrypoint: CLI (clap), env vars, starts axum server
crates/
  protocol/           # Shared types: LogEntry, WSMessage, LogLevel, detect_level(), try_parse_timestamp()
  tail-engine/        # File watching (notify), incremental LineIndex (memmap2), TailSession
  search-engine/      # grep-based search (grep-regex/grep-searcher), LogFilter (precompiled regex)
  server/             # Axum app: REST API, WebSocket handler, static file serving
frontend/             # Vue 3 + TypeScript + Vite SPA
```

- `crates/server` is the hub — depends on all other crates, owns `AppState` and `app()` router factory.
- `crates/protocol` has zero internal deps; all other crates depend on it. Contains shared utility functions (`detect_level`, `try_parse_timestamp`, `try_parse_json_fields`).
- `crates/tail-engine` uses `notify` for inotify + polling fallback; `TailSession` tracks file offset/inode for log-rotate awareness.
- `crates/search-engine` uses `memmap2` + `grep-regex` for fast file search. `LogFilter` compiles regex once via builder pattern.

## CLI

```bash
tailr --log /var/log/app /var/log/nginx /path/to/specific.log
tailr -l /var/log/app -b :8080
```

Priority: CLI args > `TAILR_LOG_DIR` env var > `<exe_dir>/logs`.

## Build

Frontend dist is **committed** and embedded into the Rust binary at compile time via `include_dir!("$CARGO_MANIFEST_DIR/../../frontend/dist")`.

```bash
make frontend          # npm install + npm run build
make build             # frontend + cargo build --release
make dev               # cargo run
make check             # cargo check
```

If `frontend/dist` doesn't exist or is stale, the server serves a placeholder HTML page.

## Linux cross-compilation

Uses Docker with musl for static binaries (no glibc dependency):

```bash
make build-linux       # x86_64
make build-linux-arm   # aarch64
make release           # frontend + both Linux targets
```

## Dev workflow

```bash
# Terminal 1: Rust backend
cargo run              # starts on 0.0.0.0:3000

# Terminal 2: Vite dev server (with proxy)
cd frontend && npm run dev   # starts on :5173, proxies /api and /ws to :3000
```

Vite proxies `/api` → `http://localhost:3000` and `/ws` → `ws://localhost:3000`.

## Environment variables

| Variable | Default | Notes |
|---|---|---|
| `TAILR_LOG_DIR` | `<exe_dir>/logs` | Comma-separated list of directories (fallback if no CLI args) |
| `TAILR_BIND` | `0.0.0.0:3000` | Listen address |
| `RUST_LOG` | — | Standard tracing env filter |

## Testing

```bash
cargo test                          # all workspace tests
cargo test -p tailr-tail-engine     # single crate
cargo test -p tailr-search-engine test_literal_search_basic  # single test
```

Tests use `tempfile::NamedTempFile` for fixtures. No external services required.

## Key conventions

- JSON field casing: `camelCase` everywhere (serde `rename_all`).
- WS protocol: tagged enum via `serde(tag = "type", rename_all = "camelCase")`.
- `Cargo.lock` is gitignored (binary crate, not a library).
- No `rustfmt.toml` or `clippy.toml` — use defaults.
- `LineIndex` and `SearchEngine` use memory-mapped files; test on files small enough for `tempfile`.
- `detect_level` uses zero-alloc ASCII comparison (`contains_case_insensitive`), no heap allocation.
- `LogFilter` uses builder pattern (`with_pattern`, `with_levels`, `with_time`), precompiles regex.
- File browser filters non-text files by extension + null-byte detection; skips empty directories (recursion depth ≤ 2).

## API surface

| Route | Method | Purpose |
|---|---|---|
| `/api/files` | GET | List log files (filtered: text files only, no empty dirs) |
| `/api/file/content` | GET | Paginated file content (offset/limit) |
| `/api/file/tail` | GET | Last N lines |
| `/api/file/info` | GET | File metadata + line count |
| `/api/search` | GET | Grep search with context, level/time filters |
| `/api/health` | GET | Status + uptime |
| `/ws` | WS | Subscribe/unsubscribe to live file tail (batched entries) |

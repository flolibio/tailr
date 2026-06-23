# AGENTS.md

## What this is

tailr: a single-binary log tail/search server. Rust backend (axum) + Vue 3 frontend. Serves a web UI that tails and searches log files via WebSocket and REST.

## Architecture

```
src/main.rs           # Binary entrypoint: CLI (clap), subcommand dispatch
src/config.rs         # Config loading: figment-based TOML/env/CLI merging, auto-init
src/daemon.rs         # Daemonization, PID file, signal handling, service file generation
crates/
  protocol/           # Shared types: LogEntry, WSMessage, LogLevel, detect_level(), try_parse_timestamp()
  tail-engine/        # File watching (notify), incremental LineIndex (memmap2), TailSession
  search-engine/      # grep-based search (grep-regex/grep-searcher), LogFilter (precompiled regex)
  server/             # Axum app: REST API, WebSocket handler, static file serving
frontend/             # Vue 3 + TypeScript + Vite SPA
  composables/        # useLogLevels, useLogStream, useAuth
  components/         # Settings UI (SettingsDialog, LogLevelSettings, TokenDialog)
  services/           # api.ts, websocket.ts
```

- `crates/server` is the hub — depends on all other crates, owns `AppState` and `app()` router factory.
- `crates/protocol` has zero internal deps; all other crates depend on it. Contains shared utility functions (`detect_level`, `try_parse_timestamp`, `try_parse_json_fields`).
- `crates/tail-engine` uses `notify` for inotify + polling fallback; `TailSession` tracks file offset/inode for log-rotate awareness.
- `crates/search-engine` uses `memmap2` + `grep-regex` for fast file search. `LogFilter` compiles regex once via builder pattern.

## CLI

```bash
tailr -l /var/log/app /var/log/nginx /path/to/specific.log
tailr -l /var/log/app -b :8080
tailr init          # Initialize config file (prompt to confirm if file exists)
tailr config        # Print config file contents
tailr stop          # Stop daemon
tailr status        # Show daemon status
tailr systemd -l /var/log/app
tailr launchd -l /var/log/app
tailr upgrade       # Self-upgrade
```

Priority: CLI args > Config file (`~/.config/tailr/config.toml`) > Env vars > Defaults.

## Build

Frontend dist is **gitignored** and built on demand. It is embedded into the Rust binary at compile time via `include_dir!("$CARGO_MANIFEST_DIR/../../frontend/dist")`.

```bash
make frontend          # npm ci + npm run build
make build             # frontend + cargo build --release
make dev               # cargo run (run `make frontend` first, or use Vite dev server)
make check             # cargo check (run `make frontend` first)
make test              # cargo test + clippy + vue-tsc
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
cargo run              # starts on 0.0.0.0:7700

# Terminal 2: Vite dev server (with proxy)
cd frontend && npm run dev   # starts on :5173, proxies /api and /ws to :7700
```

Vite proxies `/api` → `http://localhost:7700` and `/ws` → `ws://localhost:7700`.

## Environment variables

| Variable | Default | Notes |
|---|---|---|
| `TAILR_LOG_DIR` | `<exe_dir>/logs` | Comma-separated list of directories |
| `TAILR_BIND` | `0.0.0.0:7700` | Listen address |
| `TAILR_CONFIG` | `~/.config/tailr/config.toml` | Config file path |
| `TAILR_TOKEN` | — | Authentication token (overrides config file) |
| `RUST_LOG` | — | Standard tracing env filter |

## Security

### Token Authentication (optional)

```toml
# config.toml
token = ""  # empty = no auth; set to enable Bearer token auth
```

When token is set:
- All requests require `Authorization: Bearer <token>` header
- POST endpoints also require `X-Requested-With: XMLHttpRequest` header (CSRF protection)
- Frontend shows token input dialog on 401 response

### Path Validation

All file endpoints validate paths against configured `log_dirs` and `log_files` using `canonicalize()` + allowlist check. Prevents path traversal attacks.

### CORS

Restricted to: `Authorization`, `Content-Type`, `X-Requested-With` headers. Methods: GET, POST only.

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
- Frontend uses `useAuth` composable for token management (localStorage key: `tailr-token`).

## API surface

| Route | Method | Purpose |
|---|---|---|
| `/api/files` | GET | List log files (filtered: text files only, no empty dirs) |
| `/api/file/content` | GET | Paginated file content (offset/limit) |
| `/api/file/tail` | GET | Last N lines |
| `/api/file/info` | GET | File metadata + line count |
| `/api/search` | GET | Grep search with context, level/time filters |
| `/api/config/log-levels` | GET | Get current log level configuration |
| `/api/config/log-levels` | POST | Save log level configuration (requires CSRF header when token set) |
| `/api/health` | GET | Status + uptime + version |
| `/ws` | WS | Subscribe/unsubscribe to live file tail (batched entries) |

## Development Rules

### Branch Strategy

**NEVER develop features directly on main.** Always use feature branches or worktrees:

```bash
# Create feature branch
git checkout -b feat/feature-name

# Or use worktree
git worktree add ../tailr-feature feat/feature-name
```

Branch naming:
- `feat/description` — new features
- `fix/description` — bug fixes
- `refactor/description` — refactoring
- `docs/description` — documentation

### UX-First Design

When planning any feature, consider:
1. **User workflow**: How does this fit into the user's existing workflow?
2. **Discoverability**: Can users find this feature without reading docs?
3. **Feedback**: Does the user know what's happening at each step?
4. **Error recovery**: Can users recover from mistakes easily?
5. **Performance**: Does this feel responsive (< 100ms for UI interactions)?

## Version Release

Semantic Versioning (SemVer):
- **PATCH** (0.1.x): Bug fixes, no new features
- **MINOR** (0.x.0): New features, backward-compatible
- **MAJOR** (x.0.0): Breaking changes

Release workflow (on main after PR merge):
```bash
# 1. Update version in Cargo.toml and crates/server/Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit
git add -A && git commit -m "vX.Y.Z: description"

# 4. Tag and push
git tag -a vX.Y.Z -m "vX.Y.Z: description"
git push && git push origin vX.Y.Z
```

GitHub Actions creates draft release automatically. **DO NOT** use `gh release create` — let CI handle it.

## Knowledge Base

Project documentation and planning:
- Security audit: `docs/安全审计与修复方案.md`
- Feature brainstorm: `docs/功能与体验头脑风暴.md`
- Web UI upgrade plan: `docs/Web-UI自升级功能规划.md`
- GitHub CLI reference: `docs/Github操作.md`

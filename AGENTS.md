# AGENTS.md

## What this is

tailr: a single-binary log tail/search server. Rust backend (axum) + Vue 3 frontend. Serves a web UI that tails and searches log files via WebSocket and REST.

## Architecture

```
src/main.rs           # Binary entrypoint: CLI (clap), subcommand dispatch, tokio runtime + axum::serve
crates/
  core/               # Domain core layer (v0.12): business rules, no axum/HTTP dependency
                      # config (figment TOML/env/CLI merging, auto-init)
                      # daemon (daemonization, PID, signal handling, service file generation)
                      # limits (resource limits schema + validation)
                      # runtime (RuntimeSampler: sysinfo + TTL cache, sync sample_blocking)
                      # upgrade_engine (UpgradeEngine: pure download + atomic replace)
  protocol/           # Shared types: LogEntry, WSMessage, LogLevel, detect_level(), try_parse_timestamp()
  tail-engine/        # File watching (notify), incremental LineIndex (memmap2), TailSession
  search-engine/      # LevelDetector (dynamic log-level detection from user-configured keywords)
  server/             # Web presentation layer: Axum app (REST API, WebSocket, static files)
                      # upgrade.rs: UpgradeService (Web-only wrapper around core's UpgradeEngine)
                      # runtime.rs: re-export of core's RuntimeSampler (spawn_blocking at call site)
frontend/             # Vue 3 + TypeScript + Vite SPA
  composables/        # useLogLevels, useLogStream, useAuth
  components/         # Settings UI (SettingsDialog, LogLevelSettings, TokenDialog)
  services/           # api.ts, websocket.ts
```

**Three-layer architecture (v0.12+):**
- **Presentation layer** (`src/main.rs` CLI, `crates/server` Web): adapts core output to its format (terminal text / HTTP+JSON). Owns runtime orchestration (tokio runtime, `spawn_blocking`, background loops).
- **Domain core layer** (`crates/core`): tailr's business rules — config, daemon, runtime sampling, upgrade engine. No `axum`/HTTP dependency, no global runtime, no terminal I/O. Pure computation is synchronous (`fn`, not `async fn`); callers wrap in `spawn_blocking`.
- **Base capability layer** (`crates/protocol`, `crates/tail-engine`, `crates/search-engine`): atomic capabilities (shared types, file watching/indexing, level detection).

- `crates/core` depends on `protocol`; holds config schema, daemon process management, runtime sampler, and upgrade engine. Presentation layers depend on core and adapt its output.
- `crates/server` is the Web hub — depends on core + all base capability crates, owns `AppState` and `app()` router factory.
- `crates/protocol` has zero internal deps; all other crates depend on it. Contains shared utility functions (`detect_level`, `try_parse_timestamp`, `try_parse_json_fields`).
- `crates/tail-engine` uses `notify` for inotify + polling fallback; `TailSession` tracks file offset/inode for log-rotate awareness.
- `crates/search-engine` provides `LevelDetector` (zero-alloc ASCII keyword matching from user-configured levels). The grep/filter code was removed in v0.10.0 as dead code; multi-file search is planned for a future version as a fresh design.

## Architecture Rules

> Long-term constraints for the codebase. Any PR that touches crate structure, error handling, or public surfaces must check against these. v0.12 introduces the `core` layer and freezes the public API surface; rules marked **(v0.12+)** take effect then.

### Layering & dependencies

1. **Upper layers depend on lower layers, never the reverse.** The dependency graph is a DAG — `cargo` enforces this physically (cycles fail to compile). Logical order: presentation layer (CLI/Web/MCP) → domain core layer (`core`) → base capability layer (`protocol`/`tail-engine`/`search-engine`).

2. **The domain core layer (`crates/core`, v0.12+) holds tailr's business rules.** It answers "what can tailr do", not "how to deliver the result". Contains: upgrade engine (download+replace algorithm), runtime sampler (resource sampling), future search scheduler, config schema/loading/migration, daemon process management.

3. **The core layer must not contain presentation-layer-specific logic.** Not "no business logic" — core *is* the business logic. The rule is: no HTTP routing, no CLI argument parsing, no WS broadcast scheduling, no terminal I/O. These belong to their respective presentation layers.

4. **Base capability layer (`protocol`) has zero internal deps.** All other crates depend on it; it depends on nothing inside the workspace. `protocol` holds shared types and pure utility functions only.

5. **Presentation layers do not bypass the core layer.** Although CLI could physically depend on `tail-engine` directly, `core` is the single domain entry point. Presentation layers adapt core's output to their format (CLI→terminal text, Web→HTTP+JSON, MCP→stdio protocol).

6. **Core purity takes priority over code reuse.** Even if all presentation layers need identical logic, that logic stays in the presentation layer if it is presentation-specific — it does **not** sink into core just to avoid duplication. Core must remain free of any presentation concern. To eliminate duplication across presentation layers, use abstraction within the presentation tier (a shared `presentation-common` crate, shared helper modules, traits with default methods, etc.) — never by polluting core.

   **Litmus test — "fact vs presentation":**
   - **Fact (→ core):** what the data/error *is* — the raw timestamp value, the error code enum, the config field definitions, the error's baseline English description (like an HTTP status code's reason phrase). These describe the thing itself, independent of any audience.
   - **Presentation (→ presentation layer):** how it's *shown* — formatting a timestamp into "HH:mm:ss", translating an error to Chinese, mapping a code to HTTP status, wrapping in a colored terminal banner, stuffing into an MCP `data` field. These exist only because of *how* something is delivered to an audience.

   Core states *what*; presentation layers decide *how to show it*.

   **Why "would it exist with only one presentation layer?" is NOT a reliable test:** nearly every presentation layer shows timestamps and errors, so even pure presentation logic (formatting, translation) "exists" under that thought experiment. The real question is whether the logic describes the *thing* (fact) or the *display of the thing* (presentation) — not how many layers share it.

   *Example:* a log entry's `NaiveDateTime` is a fact (core). Formatting it as "HH:mm:ss.SSS" for display is presentation — every layer needs *some* formatting, but the specific format is presentation-layer concern. Core carries the raw value; each layer formats for its audience.

   *Trade-off note:* some duplication across independent presentation layers is acceptable and even healthy — it keeps layers decoupled and lets each evolve for its audience (CLI adds color, Web adds i18n, MCP adds protocol fields). Reach for a `presentation-common` abstraction only when the duplication becomes a real maintenance burden (typically 3+ layers with large shared mapping tables), not at the first sign of repetition.

### Core layer boundary (v0.12+)

The core layer is the load-bearing wall. Its boundary must be defended against creep. When adding anything to `crates/core`, check:

- ❌ **No `axum` / `tower-http` / `tower-governor` deps.** This is the hard line between core and the Web presentation layer.
- ❌ **No HTTP concepts** (`StatusCode`, `IntoResponse`, `HeaderMap`). HTTP is the Web layer's transport detail; core errors implement `std::error::Error` and stay transport-neutral.
- ❌ **No global runtime** (no `tokio::runtime::Runtime`, no process-wide singleton). The runtime is owned and started by the presentation layer (the binary).
- ❌ **No network listening / no ports / no connection accepting.** That is `server`'s job.
- ❌ **No terminal I/O** (no `println!`/`eprintln!`, no stdin reads). Terminal is the CLI layer's output channel; core output would pollute MCP's stdio protocol and Web logs.
- ❌ **No hardcoded business policy** (e.g. "check upgrade every 6h", "WS cap 50" baked into loops). Such thresholds are runtime config or presentation-layer orchestration.

**Gray areas — decide per case by "is this algorithm or orchestration":**

- **Sync vs async:** Pure computation in core stays *synchronous* (`fn`, not `async fn`). Callers decide whether to wrap in `spawn_blocking` / async runtime. `UpgradeEngine::check_update`, `LineIndex::build`, `RuntimeSampler` sampling math are sync fns in core. `spawn_blocking(...)` wrappers and background polling loops stay in the Web layer (`UpgradeService`).
- **Concurrency primitives:** `tokio::sync::Mutex`/`RwLock` are allowed in core (generic primitives, don't require a running runtime). `tokio::spawn` / `tokio::task::spawn_blocking` are forbidden in core (require runtime = orchestration).
- **I/O:** Core domain logic uses `std::fs` (sync, consistent with "computation stays sync"). `tokio::fs` only appears in the base capability layer (`tail-engine`'s async `TailSession`), not in core domain logic.
- **Config defaults:** Core defines the schema (field names, types, validation). Default values live in `Config::default()` — these are factory defaults overridable via config.toml, not hardcoded policy.

### Error handling (v0.12+)

**Core returns a fixed enum + a baseline description; presentation layers decide how to display it.** The split is "fact vs presentation":

- **Core exposes neutral errors.** Core defines the `ErrorCode` enum (single source of truth for which codes exist) + a `Display` impl giving each code a baseline English description (a domain fact, like HTTP status code's reason phrase — *not* a UI string). Core functions return `Result<T, E>` where `E: std::error::Error`. No HTTP status, no `IntoResponse`.

- **Presentation layers own differentiation.** Each layer decides how to render the error for its audience:
  - **CLI:** uses core's baseline description directly (or adds terminal color/formatting). Keeps its own user-friendly copy where it adds value (e.g. platform-unsupported message with GitHub download link).
  - **Web:** maps `ErrorCode` → HTTP status + i18n key. Multilingual — the whole point of this layer. The baseline English from core is the fallback, not the product.
  - **MCP (future):** maps `ErrorCode` → JSON-RPC error object per MCP spec.

- **The litmus test: "fact statement" vs "presentation instruction".** A `Display` returning "no release available for this platform" is a fact statement about the error (belongs in core). A translation, a colored terminal banner, an MCP `data` field — those are presentation instructions (belong in their layer). Core states *what* the error is; presentation layers decide *how to show it*.

- **HTTP-ization is the Web layer's job.** `api.rs` maps core errors to `ApiError` (with HTTP status) at the transport boundary. This mapping table is presentation-layer logic, not core.

- **Error response shape is frozen at v1.0.** `{success:false, error:{code, message}}` where `code` is SCREAMING_SNAKE. Post-v1.0, error codes are add-only (new codes ok, never rename/remove/revalue).

### Public surface freeze (v1.0+)

tailr has four public surfaces: **REST API**, **WS protocol**, **config.toml schema**, **CLI**. Post-v1.0, changes to these must be **additive only**:

- ✅ Add new fields (keep old ones, optionally alias)
- ✅ Add new endpoints (keep old ones working)
- ✅ Add new config keys (with defaults so old configs still load)
- ❌ Never delete/rename fields, reverse semantics, or remove endpoints
- ❌ Never change CLI subcommand names or remove required args

Internal restructuring (crate splits, module moves, refactors) does **not** touch public surfaces and is always allowed — including across v1.0.

### Build & dependency rules

- **Single binary, zero external runtime deps.** No dependency that requires a system shared library at runtime. Use `rustls` (not OpenSSL), `miniz_oxide` (not zlib/system flate2). This preserves the "zero user-side install" promise on alpine/distroless/scratch.
- **`tokio::fs` is allowed only in `tail-engine`** (async incremental reads). Domain logic in core and request handlers use `std::fs` or `tokio::task::spawn_blocking(std::fs::...)` for blocking file ops.
- **Tests live alongside code.** Rust convention: `#[cfg(test)] mod tests` at the bottom of each source file. Integration tests (cross-module, via the public API) go in `tests/` directories. No separate test modules.
- **`Cargo.lock` is committed.** Binary crate — ensures reproducible CI/release builds.

### When in doubt

If a change seems to require breaking any rule above, that's a signal to stop and reconsider the design, not to carve out an exception. Rules evolve, but only through explicit discussion — never by silent drift.

## CLI

```bash
tailr -l /var/log/app /var/log/nginx /path/to/specific.log
tailr -l /var/log/app -b :8080
tailr init          # Initialize config file (prompt to confirm if file exists)
tailr config        # Print config file contents
tailr stop          # Stop daemon
tailr restart       # Restart daemon (stop + re-exec with same args; supervisor-aware)
tailr status        # Show daemon status
tailr systemd -l /var/log/app
tailr launchd -l /var/log/app
tailr upgrade       # Self-upgrade (delegates to UpgradeEngine)
```

Priority: CLI args > Config file (`~/.tailr/config.toml`) > Env vars > Defaults.

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
| `TAILR_CONFIG` | `~/.tailr/config.toml` | Config file path |
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
cargo test -p tailr-search-engine   # LevelDetector tests
```

Tests use `tempfile::NamedTempFile` for fixtures. No external services required.

## Key conventions

- JSON field casing: `camelCase` everywhere (serde `rename_all`).
- WS protocol: tagged enum via `serde(tag = "type", rename_all = "camelCase")`.
- `Cargo.lock` is committed to version control (binary crate, ensures reproducible CI/release builds).
- No `rustfmt.toml` or `clippy.toml` — use defaults.
- `LineIndex` uses memory-mapped files; test on files small enough for `tempfile`.
- `detect_level` uses zero-alloc ASCII comparison (`contains_case_insensitive`), no heap allocation.
- File browser filters non-text files by extension + null-byte detection; skips empty directories (recursion depth ≤ 2).
- Frontend uses `useAuth` composable for token management (localStorage key: `tailr-token`).

### i18n: 加 key 后必须做的事(踩坑记录)

`@intlify/unplugin-vue-i18n` 的 Vite HMR 对 `src/locales/*.json` 的改动**不可靠**——新增 key 后,dev server 经常不刷新消息表,运行时 `t()` 返回 key 原文(如页面显示 `settings.updateDetected` 而非翻译)。此问题已反复出现(`invalidToken`、`systemFontLabel`、`updateDetected` 等)。

**加 i18n key 的标准流程**:
1. 同时改 `zh-CN.json` 和 `en-US.json`(两个文件 key 必须一致)
2. 运行 `cd frontend && npm run check:i18n` 静态校验(代码引用 vs JSON key 完整性 + 两语言一致性)
3. 如果 dev server 已在跑,**重启它**(`Ctrl+C` 后重新 `npm run dev`)才能加载新 key
4. 生产构建(`npm run build`)不受影响——只有 dev HMR 有这个问题

**CI 已集成** `check:i18n` 步骤,PR/push 时自动检查;缺失或不一致会 fail。

## API surface

| Route | Method | Purpose |
|---|---|---|
| `/api/files` | GET | List log files (filtered: text files only, no empty dirs) |
| `/api/file/tail` | GET | Last N lines |
| `/api/config/log-levels` | GET | Get current log level configuration |
| `/api/config/log-levels` | POST | Save log level configuration (requires CSRF header when token set) |
| `/api/upgrade/check` | GET | Check for newer release (read-only; returns `supported` platform flag) |
| `/api/upgrade` | POST | Download + replace binary + delegate restart. **Forced auth**: requires non-empty token even when global auth is disabled (binary replacement is RCE-class), plus `X-Requested-With` CSRF header |
| `/api/health` | GET | Status + uptime + version. **Exempt from rate limiter** (read-only, polled by LB probes) |
| `/api/runtime` | GET | Runtime resource snapshot (process/system CPU+memory, disk, WS connections, uptime). TTL-cached 5s; refresh runs in `spawn_blocking`. **Exempt from rate limiter** (read-only, polled by Runtime panel) |
| `/api/docs/openapi.json` | GET | OpenAPI 3.0 spec (machine-readable API contract). **Exempt from rate limiter**. Render at editor.swagger.io — no swagger-ui bundled |
| `/ws` | WS | Subscribe/unsubscribe to live file tail (batched entries) |

### Error response format (v0.12+)

All errors return HTTP 4xx/5xx + body `{success:false, error:{code, message}}`:
- `code` — SCREAMING_SNAKE machine-readable identifier (stable, add-only after v1.0). Defined in `tailr_core::error::ErrorCode`.
- `message` — baseline English (frontend maps `code` → i18n key; this is the fallback).

Success responses return HTTP 200 + `{success:true, data:<T>}`.

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

### Git conventions

- **Commit messages, PR titles and PR descriptions MUST be in English.** Keep historical records as-is (never rewrite pushed history); this rule applies to all new commits and PRs going forward.
- Branch naming and tag format follow the patterns already used in this doc (`feat/...`, `fix/...`, `vX.Y.Z`).

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

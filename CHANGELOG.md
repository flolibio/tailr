# Changelog

## [v1.0.3] - 2026-08-11

### Fixes

- **Cross-major version upgrades blocked by `self_update`'s compatibility filter** — `self_update::ReleaseUpdate::update()` internally filters release candidates with `bump_is_compatible`, which follows cargo's semver rule that `0.x → 1.0` is a breaking change. This made every v0.12 binary stall at v0.12.4 when trying to auto-upgrade: v1.0.x releases were silently filtered out, leaving only v0.12.3/v0.12.4 as "compatible" candidates. Fixed by bypassing `ReleaseUpdate::update()` entirely and implementing the download+replace flow with `self_update`'s low-level APIs (`ReleaseList` + `Download` + `Extract` + `self_replace`), using a plain semver `>` comparison (`semver::Version`) that allows any upward jump. **Note for v0.12.x users**: this fix ships in v1.0.3, which v0.12.x binaries cannot auto-install (chicken-and-egg). v0.12.x users need one manual upgrade: download `tailr-x86_64-linux-musl.tar.gz` (or aarch64) from the [latest release](https://github.com/flolibio/tailr/releases/latest), extract, and replace the binary, then `tailr restart`. After that, auto-upgrade works normally for all future versions.

## [v1.0.2] - 2026-08-11

### Fixes

- **Upgrade engine reported a stale version** — `UpgradeEngine::new()` read `env!("CARGO_PKG_VERSION")` inside `crates/core`, which expands to **core's** crate version (`0.12.2`), not the binary's (`1.0.1`). The upgrade checker therefore compared incoming releases against `0.12.x`, so after an "upgrade" to 1.0.1 the reported current version stayed at `0.12.x`. Fixed by injecting the binary version at the call sites (`UpgradeEngine::with_version(env!("CARGO_PKG_VERSION"))` in both `main.rs` and `crates/server/upgrade.rs`, where `env!` resolves to the binary/server version). Also bumped `tailr-core` to `1.0.1` for consistency.
- **Web upgrade hung the process if the page was closed mid-upgrade** — the download, cache invalidation, and restart scheduling were all awaited inside the HTTP request future. When the browser tab closed, axum dropped the future: the binary had already been replaced on disk, but `spawn_restart()` never ran and the process was stuck — no restart, and no way to retry. The upgrade is now a fully **detached background task** that survives client disconnect; the handler returns immediately with `status:"started"`. Added a 5-minute timeout around the blocking `self_update` call so a stalled GitHub download can't occupy a blocking-pool thread forever.
- **Removed the forced-token restriction on WebUI upgrade** — the `/api/upgrade` endpoint used to reject requests with `TOKEN_REQUIRED` when no token was configured, even though global auth was disabled. This gate is removed; upgrade now follows the same auth policy as every other endpoint (Bearer auth when a token is set, open otherwise). The `X-Requested-With` CSRF header remains required unconditionally. `ErrorCode::TokenRequired` is retained (v1.0 add-only contract) but no longer emitted by this endpoint.

## [v1.0.1] - 2026-08-09

### Fixes

- **displayMode fallback defaults aligned to 'cozy'** — `LogPanel.vue` and `LogViewer.vue` both fell back to `'compact'` when the `displayMode` prop was absent, inconsistent with `App.vue`'s `defaultSettings` (`'cozy'`). First-time users already got cozy via the normal prop-passing flow; this fixes the fallbacks so any caller without an explicit prop also lands on cozy.

## [v1.0.0] - 2026-08-09

**First stable release. Public surfaces (REST API, WebSocket protocol, config.toml schema, CLI) are now frozen — post-1.0 changes are additive only.**

### Stability & freeze

- **Public API surface frozen.** The 10 REST endpoints, WS protocol, config.toml schema, and CLI subcommands are now under a stability contract: new endpoints/fields/keys may be added, but existing ones will not be removed, renamed, or have their semantics reversed.
- **Error code contract frozen.** The 11 `ErrorCode` variants and the `{success:false, error:{code, message}}` response shape are now add-only.

### Pre-freeze cleanup

- **Fix TCP bind panic** (`src/main.rs`): `TcpListener::bind().unwrap()` replaced with a graceful error path. Port-in-use (e.g. an old daemon still running) now prints a clear message + `tailr stop` hint and exits 1 instead of panicking.
- **README sync**: API table now lists all 10 endpoints (`/api/runtime` and `/api/docs/openapi.json` were missing); architecture section updated to the v0.12 three-layer layout (`crates/core` added, `src/config.rs` removed, grep reference dropped); rate-limit burst corrected (`×3` → `×10`); `/api/config/log-levels` POST description fixed (hot-reload only, no longer persists to config.toml).

### What's included (since v0.12.0)

- v0.12.0: three-layer architecture (`crates/core`), unified error format (11 codes), OpenAPI endpoint
- v0.12.1: `save_log_levels` no longer rewrites config.toml (stops stripping comments); SelectionToolbar z-index fix
- v0.12.2: upgrade UX (drop confirm dialog, release-notes link, friendlier token error)
- v0.12.3: FilterBar chip overflow "+N" folding + suggestions dropdown keyboard navigation/match highlighting
- v0.12.4: FilterBar UI cleanup (remove status-bar keyword text, remove chip inline-edit, fix descender clipping, visual polish)

## [v0.12.4] - 2026-08-07

### FilterBar UI cleanup and refinements

- **Remove status bar keyword text** — the `N matches · kw1 + kw2 + ...` line was too long to fit the status bar. Drops the now-dead `matchCount` computed, `.status-filter-info` CSS, and `app.matches` i18n key.
- **Remove chip inline-edit (double-click)** — editing was only reachable via dblclick and added complexity (emit, handler, state, styles). Users edit by removing + re-adding. Drops `editKeyword` emit/handler, editing state, and `.chip-edit-input` styles.
- **Fix chip-text descender clipping** — `.chip` and `.chip-text` now share `line-height: 1.2` (was 1 on the container, 1.2 on the text). At `1`, glyph descenders (g/j/p/q/y) exceeded the box and got clipped by `overflow:hidden` (scrollHeight 14 > clientHeight 13).
- **Visual polish** — chip font-size 12 → 13, softer kw-chip bg/border opacity, suggestion history icon 13 → 14 with `text-2` color, suggestion item 14px non-mono, stronger match highlight (18% → 40%), lighter in-log `mark.kw-mark` background.

## [v0.12.3] - 2026-08-07

### FilterBar

- **Fix: filter-content no longer widens beyond the viewport.** Root cause was a broken flex-shrink chain — `.filter-wrap` lacked `min-width:0`, so chip content pushed the whole row wider than the screen. Added `min-width:0` across `.filterbar` (grid item), `.filter-bar` (component root), and `.filter-wrap` (the actual break point).
- **Chip overflow folds into a "+N" badge.** When chips don't fit, the oldest are folded (front-collapsing: newest chips stay visible next to the input). Uses a scrollWidth-measuring probe + binary search to find the fold count — no magic constants, unlike the earlier failed budget-estimation approach. Clicking the badge opens a popover (width-aligned to the search box) listing the hidden chips, where they can be removed.
- **Keyword chip interactions (learned from the demo):**
  - `title` attribute on chips — hover shows the full keyword text when truncated.
  - Enter **and** Space both add a keyword.
  - Pasting comma/space-separated text auto-splits into multiple chips.
  - Chips restyled to pill shape (`border-radius:9999px`); remove button is now circular with hover-invert.
  - New chips animate in with a scale "pop".
  - `level-tag` buttons also restyled to pills for visual consistency.
  - Light-mode `--kw-1` hue adjusted (43 → 25).

### Suggestions dropdown

- **Width now aligns with the search box** (`right:0` instead of `right:30px`).
- **Keyboard navigation:** ArrowUp/ArrowDown cycle the highlight (scrollIntoView keeps it visible), Enter/Tab commit the highlighted item (or the input text if none highlighted), Escape closes. The first item is highlighted by default so Enter can commit immediately.
- **Match highlighting:** the matched substring is wrapped in `<mark>` (case-insensitive, supports mid-word matches, XSS-safe via escaping).
- **Visual refresh (v3 minimal-tech style):** history icon (`↻` glyph → lucide `History` SVG), softer hover/active background, highlighted match gets a tinted background.

### Docs

- **Git convention:** commit messages, PR titles and PR descriptions must be in English going forward (historical records untouched).
- Added `docs/feat/chip-search-demo-v{2,3,4}.html` as design references.

## [v0.12.2] - 2026-08-05

### UX

- **去掉升级确认弹窗：** 点击"升级"按钮不再弹出 `Upgrade to vX.Y.Z? The service will restart. Continue?` 确认框，直接开始升级流程（面板内已有升级中/重启进度反馈，二次确认冗余）。
- **升级面板增加"查看完整发布说明"链接：** 检测到新版本时，面板在版本号下方显示固定的"查看完整发布说明"链接，指向 GitHub release 页面，用户可自行跳转查看更新内容（统一交互，不直接渲染 markdown 源码）。
- **Token 错误提示友好化：** 未配置 Token 时点升级，提示从"升级未启用：替换二进制需要鉴权..."改为"升级功能需要先配置访问令牌（Token）。请在「通用设置」中设置后重试。"——去掉"替换二进制/鉴权"等技术黑话，保留用户可操作信息。

## [v0.12.1] - 2026-08-04

### Fixes

- **save_log_levels no longer rewrites config.toml:** the handler was reading config.toml, inserting the log levels, and writing it back via TOML round-trip — which silently stripped all user comments (TOML standard: comments are display-only, not part of the data model). Now it only hot-updates the runtime level detector + in-memory config. The UI button label changed from "保存/Save" to "应用/Apply" to reflect the new semantics. Permanent changes require editing config.toml and restarting.
- **SelectionToolbar z-index:** the floating action bar (z-index 9999) was stacked above modal dialogs (SettingsDialog z-index 1000), causing it to overlap the settings window when both were visible. Lowered to z-index 100 (above content, below modals).

## [v0.12.0] - 2026-08-04

### Architecture

- **Three-layer architecture:** introduces a domain core layer (`crates/core`) between the presentation layer (CLI/Web) and the base capability layer (`protocol`/`tail-engine`/`search-engine`). This is a pure internal refactor — CLI subcommands, REST API, WS protocol, and config schema are unchanged. It sets up a clean foundation for future CLI log search and MCP integration (both can now reuse core logic without pulling in the axum HTTP stack).
  - `limits`, `config`, `daemon` moved from `src/` and `crates/server/` into `crates/core`
  - `runtime` (RuntimeSampler) moved to core as a synchronous `sample_blocking`; the Web layer wraps it in `spawn_blocking` at the call site
  - `UpgradeEngine` (pure download + replace) moved to core; `UpgradeService` (Web-only, with restart delegation) stays in server
  - The cyclic-dep workaround (`config.rs` `pub use tailr_server::LimitsConfig`) is eliminated — `LimitsConfig` and `Config` now share a crate
  - Core has zero `axum`/HTTP dependency, no global runtime, no terminal I/O. Architecture rules documenting the boundary contract are in `AGENTS.md`

### Breaking Changes

- **Unified error response format:** all API errors now return HTTP 4xx/5xx + `{success:false, error:{code, message}}` instead of the previous mixed signals (HTTP 200 + `{success:false, error:"string"}` coexisting with bare HTTP status codes). The `code` field is a stable SCREAMING_SNAKE identifier (e.g. `NOT_FOUND`, `UNAUTHORIZED`, `RATE_LIMITED`); `message` is the baseline English fallback (the frontend maps `code` to localized i18n strings).
  - 11 error codes defined in `tailr_core::error::ErrorCode` (single source of truth)
  - The Web layer (`ApiError`) maps each code to an HTTP status at the transport boundary
  - Frontend `api.ts` simplified: the `json.success === false` branch is removed; branch order preserved (401 → token dialog, 429 → backoff retry, then generic error)
  - This is the last breaking change before the v1.0 public surface freeze

### Features

- **OpenAPI spec endpoint:** `GET /api/docs/openapi.json` serves a machine-readable OpenAPI 3.1.0 spec (via utoipa). No swagger-ui is bundled — paste the URL into editor.swagger.io to render. This is the v1.0 freeze precondition ("public API has formal documentation"). Exempt from rate limiting.
- **Config template improvement:** the default `config.toml` template now keeps `[limits]` and `[daemon]` section headers open (child values commented). Previously both the header and children were commented, creating a trap where uncommenting a child without the header silently parsed it as a top-level orphan that figment ignored.

### Fixes

- **Startup limits logging:** the server now logs the effective limits (`ws_cap`, `rps`, `workers`, `compression`) at startup, so configuration issues are visible at a glance.

### Tests

- **API layer integration tests:** 14 integration tests added (axum `oneshot`, in-process, no TCP listener), covering all 7 REST endpoints × (success + main error codes) + the frozen response body shape contract. Previously the entire HTTP/WS request layer had zero tests.

## [v0.11.1] - 2026-07-24

### Fixes

- **Process CPU always shows 0.0%:** sysinfo's `refresh_processes_specifics(Some(&[pid]))` does NOT compute per-process CPU% — `compute_cpu_usage` is only called for `ProcessesToUpdate::All` (sysinfo 0.32 source limitation). On CentOS 6 / kernel 2.6.32 (and likely all platforms), this left `Process::cpu_usage()` permanently at its initial value (~0). Now reads `/proc/[pid]/stat` directly and diffs utime+stime between samples (exactly what `top` does), giving accurate process CPU% on Linux. Verified: `top` showed 0.3-0.7% while sysinfo reported 5.97e-7% — now matches.
- **System Memory tooltip:** added a footnote clarifying that memory usage excludes cache (buffers/cached), reflecting actual process usage only. Prevents confusion when comparing with `free`'s first line (which counts reclaimable cache as "used").

## [v0.11.0] - 2026-07-24

### Features

- **Runtime observability (`GET /api/runtime` + Settings panel):** the server now exposes live resource metrics via a new read-only endpoint, and the Settings dialog gains a "Runtime" panel showing them. Closes the self-ops loop started in v0.9 — users no longer need SSH to check whether the service is eating memory or the log disk is full.

  The panel is split into two sections to keep semantics clear:
  - **tailr process:** process memory (RSS), process CPU %, uptime, active WebSocket connections.
  - **server:** system memory (used/total + bar), system CPU % (bar), log disk usage (bar, turns amber past 80%). A hint under this section notes that under container deployment these reflect the host machine, not the container's cgroup quota.

  Data refreshes every 5s via HTTP polling (not WS — keeps the log-stream protocol clean and stops when the panel closes). A pulsing live indicator (dot + timestamp) at the top signals each fresh sample.

  Sampling is TTL-cached (5s) on the server: repeated requests within the window return the cached snapshot without re-running `sysinfo`. The actual `refresh_*` calls run inside `spawn_blocking` so they never stall tokio workers (same pattern as the v0.10 `LineIndex::build` fix). No background thread — zero cost when nobody is viewing the panel.

  `/api/runtime` and `/api/health` are **exempt from the per-IP rate limiter** (split into `api::routes_unlimited`). Both are read-only, TTL-cached, and polled on a timer; letting them share the business endpoints' GCRA bucket meant the runtime panel's 5s poll slowly ate into the client's request budget — combined with a tab-restore burst (which exhausts the 60-request burst capacity), the panel would start hitting 429 minutes later and stay throttled for a while (GCRA recovers slowly after exhaustion). Business endpoints (`/api/files`, `/api/file/tail`, config, upgrade) remain rate-limited.

### Fixes

- **Rate limiter no longer trips on normal usage / GCRA slow-recovery mitigated:** the GCRA burst capacity was raised from `rps × 3` (= 60) to `rps × 10` (= 200). GCRA's defining trait is slow recovery after exhaustion — a fully-drained bucket takes ~20s before a single request passes again (TAT-based penalty). The old tight burst tripped on ~6 rapid page reloads (72 requests), then penalized the user with a 20-60s throttle window where every business request 429'd. Since tailr's "bursts" are normal user behavior (tab restore, impatient reloads), not abuse, this was the wrong tradeoff. At ×10, it takes ~17 consecutive 12-request reloads (204 requests in ~5s) to exhaust — far beyond any realistic non-automated usage. Genuine abuse (sustained high RPS) is still throttled. The frontend also now transparently backoff-retries transient 429s (1s → 2s → 4s with jitter) before surfacing the error, so edge-case 429s are invisible to the user.

  Uses `sysinfo` 0.32 with `default-features = false, features = ["system", "disk"]` (drops network/component/user/multithread). sysinfo calls platform base libs (libc via `/proc`+`/sys` on Linux, libSystem on macOS) — these are always-present OS libs, statically linked under musl, so the zero-install promise is preserved.

## [v0.10.2] - 2026-07-23

### Features

- **Nginx log level preset:** new `nginx` preset (8 levels: emerg → debug) with lowercase keywords matching nginx error_log format. Available in both the Web UI preset selector and the backend `default_log_levels`.

### UI

- **New-logs button redesigned:** changed from a centered accent-colored text pill to a circular icon button (ChevronDown) at bottom-right — white background, gray border, less intrusive. Same scroll-to-bottom behavior.
- **File tree indentation guides:** added vertical guide lines at each depth level in the file browser, using theme-adaptive `--border` color. Matches VS Code / file manager tree conventions.
- **Removed redundant `.app-shell` border/radius:** the border and border-radius were invisible since `.app-shell` fills 100vh with no margin. Kept `overflow: hidden`.

## [v0.10.1] - 2026-07-23

### Improvements

- **Configurable tokio worker threads:** the server no longer spawns one worker thread per CPU core by default (8-core box = 15 threads, most idle). A new `[limits] workers` option (default 2) controls the tokio async worker pool; the blocking pool (mmap index build, GitHub HTTP) is capped at 4 internally. tailr is IO-bound (log tailing + WebSocket fan-out), so 2 workers cover single-user and small-team scenarios with room to spare. Raise to 4+ for large teams; lower to 1 for memory-constrained containers. Old config files without the key still load (serde default = 2). An 8-core machine drops from ~15 to ~7 threads.

## [v0.10.0] - 2026-07-22

### Fixes

- **Self-upgrade fails after path migration:** when upgrading from 0.9.x to 0.10.x via the Web UI, `tailr restart` looked for the PID file at the new path (`~/.tailr/tailr.pid`) but the old daemon wrote it to the legacy path (`~/.local/share/tailr/tailr.pid`). The old process was never killed, the port stayed occupied, and the new process failed to bind. `stop_daemon` / `daemon_status` / `restart_daemon` now fall back to the legacy PID file path when the new one doesn't exist.

### Features

- **Production resource limits (`[limits]` config section):** new opt-in section in `config.toml` with three user-tunable thresholds. All default to safe values for the primary gigabit-LAN deployment scenario.
  - `max_ws_connections` (default 50): global WebSocket connection cap. Over-limit connections are accepted then immediately closed with code 1013 (Try Again Later) — browser WS API hides HTTP handshake status codes, so returning 429 is indistinguishable from network failure on the client.
  - `rate_limit_rps` (default 20): per-client-IP REST rate limit (GCRA, burst = rps × 3). Each LAN client gets its own bucket.
  - `enable_compression` (default false): opt-in gzip response compression. Break-even is ~560 Mbps; off by default for LAN (miniz_oxide CPU cost > transfer savings), on for public/weak-network access.
- **Per-IP REST rate limiting:** `tower_governor` (governor underneath) extracts the TCP peer IP from axum's `ConnectInfo<SocketAddr>` extension. Required `into_make_service_with_connect_info::<SocketAddr>()` on the `axum::serve(...)` call site. tailr is direct-deployed (systemd/launchd starts the binary, no reverse proxy), so TCP peer IP == real client IP — no X-Forwarded-For parsing (forgery risk).
- **Global WebSocket connection cap:** `AtomicUsize` counter with TOCTOU-safe `fetch_add` + rollback on over-limit. Counter is released in `cleanup_client` when the socket closes.
- **First-open no longer blocks the runtime:** `LineIndex::build` in `handle_subscribe` (WS subscribe) was synchronous — on a 10 GB log it blocked the tokio worker for seconds, freezing every other WS push / HTTP request / watcher poll. Now wrapped in `tokio::task::spawn_blocking`. Concurrent first-open of the same file may build twice (race widened), but build is a pure function so results are equivalent.
- **Rate-limit / WS-cap errors surfaced in the UI:** before, hitting REST 429 left the log area blank (the load catch only `console.error`'d) and a WS rejection looked identical to network failure (browser WS API hides handshake status), causing an infinite reconnect storm. REST 429 now throws `RateLimitError`, the log panel shows an error state with a Retry button, and a deduplicated toast reports the `Retry-After` hint. WS close code 1013 stops auto-reconnect and surfaces a "connection limit reached" toast with a manual retry entry.

### Architecture

- **`LimitsConfig` lives in `tailr-server`:** the server crate owns `AppState` (which consumes the limits), so the config type lives there and is re-exported from the binary crate. Avoids a cyclic dep (server can't depend on the binary).
- **CompressionLayer layering:** must be the innermost body-transforming layer (before `CorsLayer`) or it silently no-ops — verified empirically. flate2 forced to `rust_backend` (miniz_oxide) so the static binary doesn't link libz — preserves tailr's zero-install guarantee.
- **Unified data directory `~/.tailr/`:** all tailr files (config, PID, logs, restart state) now live in one directory instead of being split across `~/.config/tailr/` (XDG config) and `~/.local/share/tailr/` (XDG data). On first launch with the new version, an existing `~/.config/tailr/config.toml` is automatically copied to `~/.tailr/config.toml` (the old file is kept as backup). The `dirs` crate dependency was removed — paths are resolved from `$HOME` directly.

### Removed

- **3 dead endpoints dropped:** `GET /api/file/content`, `GET /api/file/info`, `GET /api/search` — zero frontend callers (verified via grep). Cascading cleanup deleted `grep.rs` / `filter.rs` in search-engine (the crate now only provides `LevelDetector`), the `AppState.search_engine` field, 7 now-unused structs, frontend api.ts wrappers, and orphaned workspace deps (`memchr`, `tracing-subscriber`). Multi-file search is planned for v0.12 as a fresh design (the old single-file `/api/search` wouldn't be reused anyway). Not a breaking change: these endpoints were never advertised as stable (project is 0.x).

### UI

- **Default theme is now "follow system":** new users get `prefers-color-scheme` instead of always-dark. The three-way selector (Light / Dark / System) in Settings highlights "System" on first open.
- **Default display mode is now "cozy":** was "compact".
- **Default language is now English:** was inferred from browser language. Users can still switch to zh-CN in Settings; their choice is persisted.
- **Token dialog can no longer be dismissed without a valid token:** the dialog only appears on 401 (token missing or invalid), so there's no valid prior state to dismiss to. Removed the overlay click-to-close, Cancel button, and Escape shortcut — the only way out is entering a token that passes verification. Previously, dismissing created an annoying close→reopen loop as the next API call 401'd again.

## [v0.9.5] - 2026-07-20

### Fixes

- **Post-upgrade restart target resolution:** `spawn_restart` used only `current_exe()`, which returns `/opt/tailr (deleted)` after `self_replace` overwrites the running binary (Linux marks `/proc/self/exe` with that suffix). Now `spawn_restart` prefers the exe path persisted in `tailr.cmd` at startup (clean, recorded before any replacement), and strips the `(deleted)` marker from `current_exe()` as a fallback. Eliminates the spurious first-attempt failure logged on every upgrade.
- **Update cache stale after upgrade:** `perform_upgrade` now clears the `UpgradeService` cache right after the binary is replaced, so checks between upgrade completion and restart no longer serve the stale "update available" result.
- **Check-for-updates served stale cache:** the manual "Check for updates" button read from the backend's 6h cache, hiding newly published releases. It now sends `?force=true` to bypass the cache (the background poll still uses it).
- **File browser preload broken for multiple log_dirs:** `?depth=N` only recursed in the single-log_dir case and the `?path=` case; multi-log_dir servers listed each root with empty children. Each configured `log_dir` now recurses to the requested depth.

### UI

- About: after checking for updates, the version row shows a text hint ("Update available") instead of duplicating the version delta shown in the action panel below.
- About: version number and check-for-updates button color `--text-3` → `--text-2`.
- Settings dialog height increased (580px → 680px) with a viewport cap.
- Font dropdown redesigned with `<optgroup>` categories (System / Nerd Font / Popular Monospace) using exact registered family names, plus a live font preview.

### Improvements

- **i18n key-completeness check:** `npm run check:i18n` (and a CI step) statically verify that every `t('...')` reference has a matching key in both locale files — guards against the `@intlify/unplugin-vue-i18n` HMR staleness that repeatedly caused raw keys to render (documented in AGENTS.md).

## [v0.9.4] - 2026-07-18

### Fixes

- **Post-upgrade restart may not bring the server back (daemon mode):** the `tailr restart` subprocess spawned after a binary replacement inherited the server's process group/session, so when it killed the server (its parent) the subprocess could be torn down too, leaving the server stopped. `spawn_restart` now starts the restart subprocess in its own session (`setsid`) with redirected stdio, so it survives the parent being stopped.
- **Post-upgrade restart target resolution (ENOENT):** in production, `spawn_restart` failed with "No such file or directory" because `current_exe()` returned a path that wasn't spawnable right after a binary replace. `spawn_restart` now tries multiple candidates — first `current_exe()`, then the exe path persisted in `tailr.cmd` at startup — and logs each attempt (path + exists flag) so failures are diagnosable.

### Improvements

- **Upgrade/restart observability:** the upgrade and restart paths were nearly silent in the logs, making it impossible to diagnose why a post-upgrade restart didn't happen. Added structured logging (`tracing`) at every key step: upgrade start, binary replaced, restart subprocess spawned (success/failure), restart phases (stop / wait-for-supervisor / re-exec / new-PID detected / timeout), persisted restart command, and server startup (now includes version + PID). Next time a restart misbehaves, the log tells you exactly which step failed.

## [v0.9.3] - 2026-07-18

### Fixes

- **File browser preload broken for multiple log_dirs:** `?depth=N` only recursed in the single-log_dir case and the `?path=` case. When a server monitored multiple `log_dirs` (e.g. `-l /logs/service -l /logs/php -l /logs/nginxlogs`), each root was listed with empty children — so the frontend's 3-level preload, search, and instant-expand all failed for multi-dir deployments. Each configured `log_dir` now recurses to the requested depth. (Affects v0.9.0–v0.9.2.)

### UI

- About: version number and check-for-updates button color `--text-3` → `--text-2`.

## [v0.9.2] - 2026-07-18

### Fixes

- **Check for updates returned stale results:** the manual "Check for updates" button read from the backend cache, so after a new release was published it kept reporting "Up to date" until the 6h cache TTL expired. The manual check now sends `?force=true` to bypass the cache and query GitHub directly. The background poll still uses the cache (its purpose is to stay cheap).

## [v0.9.1] - 2026-07-18

### Fixes

- **Share link on token-protected server:** three failure modes fixed. (1) A wrong token was silently stored and the dialog closed as if it succeeded — the token is now verified against `/api/health` before saving; 401 stays in the dialog with an error. (2) After entering the correct token, the log area stayed empty because the failed tab was only `switchTo`'d, not reloaded — `openTab` now re-runs `loadInitial` for a non-lazy empty tab (restores both content and WS subscription). (3) The share-link URL params were cleared too eagerly (on token change, not on load success), losing the share state on any auth failure — the URL is now cleared only once the file actually loads.

### Features

- **File browser 3-level preload:** `list_files` accepts `?depth=N` (default 1, hard-capped at 4 with a 5000-entry cap) and recurses, populating `FileEntry.children`. The frontend renders a recursive `FileTreeNode` (replaces the fixed two-level template), requests `depth=3` on root load and lazy expansion so typical log trees are visible instantly; deeper dirs stay collapsed for on-demand lazy load. The historical-file and search filters now walk the full depth. Directories default to collapsed (preload gives instant expand, not auto-expansion).
- **Font settings redesign:** the font dropdown is grouped into System / Nerd Font / Popular Monospace via `<optgroup>`, using exact registered family names (fixes silent fallback from wrong value names like `JetBrains Mono NF`). Added a live font preview showing sample log lines rendered in the selected font + size.

### UI

- Settings dialog height increased (580px → 680px) with a viewport cap.
- File browser search input uses the primary background (`--bg`) instead of gray when idle.
- Settings gear icon color `--text-3` → `--text-2` to match the adjacent share button.

## [v0.9.0] - 2026-07-16

### Features

- **Restart command:** `tailr restart` stops the running daemon and re-launches it with the original CLI args. Supervisor-aware: under systemd/launchd it relies on the unit/plist restart policy and waits for a new PID; in manual/daemonize mode it re-execs the current binary. Synchronous implementation matching `stop_daemon`'s style (no temporary runtime).
- **Web UI upgrade:** Settings → About now has a "Check for updates" / "Upgrade" flow. Upgrade delegates to the new `UpgradeService` which, on success, spawns `tailr restart` and the frontend polls `/api/health` until the server returns, then reloads.
- **Platform gating:** macOS shows the version delta and a manual download link but disables the upgrade button (automatic upgrade is Linux x86_64/aarch64 only, matching the existing CLI constraint).

### Architecture

- **Shared upgrade engine:** all `self_update` configuration now lives in `crates/server/src/upgrade.rs::UpgradeEngine` — the single source of truth used by both the CLI (`tailr upgrade`) and the Web UI (`POST /api/upgrade`). Platform judgment (`supported()`) is centralized here so the two entry points can never disagree. `self_update` moved from the root binary crate to `crates/server`; the root crate now accesses it indirectly via `tailr_server::upgrade`.
- **`UpgradeService` (Web-only):** wraps `UpgradeEngine` to add restart semantics (spawn `tailr restart` after a 1s delay). The CLI entry point bypasses this and lets the user restart manually, keeping "restart" an explicit decision outside the shared engine.

### Security

- **Forced auth on upgrade:** `POST /api/upgrade` requires a non-empty token even when global auth is disabled. Replacing the running binary is an RCE-class operation; it must never be reachable when auth is off. When the token is empty the endpoint refuses with an actionable error pointing the user to configure a token. `X-Requested-With` CSRF check applies once a token is set (same pattern as `/api/config/log-levels`).

## [v0.8.0] - 2026-07-15

### Features

- **Share link:** generate a shareable URL encoding file path + filter keywords + log levels via the Share2 button in the global bar. Opening a share link restores the exact viewing state, then cleans the URL to the root path. Subsequent tab switches and filter changes never pollute the URL.
- **Tab persistence:** open tabs and the active tab are persisted to localStorage and restored on page reload. The active tab loads immediately; others start lazy and load on first switch, avoiding unnecessary WS subscriptions and network requests.
- **Per-tab viewer state preservation:** refactored from a single `:key`-destroyed LogViewer to multi-instance `LogPanel` components kept alive with `v-show`. Switching tabs now preserves scroll position, measured row heights, expanded JSON rows, and marked lines — no save/restore machinery needed.

### Architecture

- **Multi-instance LogPanel:** each tab owns a `LogPanel` wrapping empty/loading/LogViewer states with its own `filteredEntries` computed, achieving state isolation (filtering one tab never re-renders another).
- **Shared filter logic:** extracted `filterEntries()` to `utils/filter.ts`, used by both App.vue statusbar and LogPanel viewer to prevent count/render desync.

### Fixes

- Background tab `pendingEntries` now capped at `maxLines` to prevent unbounded memory growth on high-volume logs.
- `restoreTabs` enforces `MAX_TABS` slice for defensive consistency.
- Share link URL params consumed once on load; address bar stays clean during normal use.

## [v0.7.0] - 2026-07-13

### Features

- **Multi-file tab interface:** open multiple log files side by side in a Chrome-style tab bar, each with independent filter state. Tab bar merges into the global bar; up to 10 tabs.
- **Bookmarks:** bookmark panel with line marking in the log viewer. Stale bookmarks (lines that shifted due to log rotation / buffer eviction) are detected and removed on click.
- **Recent files:** quick-access section listing recently opened files (capped at 10), persisted to localStorage.
- **Historical log filter:** toggle to show/hide logrotate-produced historical files (numbered rotation, date-named, `.bak`/`.old` markers). Hidden by default to reduce clutter.
- **Configurable log timezone:** new `log_timezone` config option (`local` | `utc` | `z` | `+HH:MM` | `+HHMM` | `+HH`) for interpreting timezone-less (naive) log timestamps. Defaults to `local` for backward compatibility.
- **JSON log timestamp parsing:** recognizes `time`/`@timestamp`/`timestamp` fields and epoch seconds/millis in JSON log lines.
- **Bracketed & two-digit-offset timestamps:** `[YYYY-MM-DD HH:MM:SS]` and ctime `+08` offset forms now parse.
- **Global bar redesign:** replaces topbar with a unified global bar — path copy, sidebar toggle, and tab strip in one place.
- **Icon system migration:** all inline SVGs replaced with `lucide-vue-next` for consistency.

### UI

- Unified color system with level-derived transparency layering.
- Chrome-style tab bar with rounded "ear" corners; inset hover pill on inactive tabs; padding prevents first/last tab ear clipping.
- Hover-reveal patterns for nav-time, file-size, and sidebar toggle.
- Bookmark panel styling; file/folder icons in FileBrowser.
- Horizontal action buttons in compact mode; increased initial tail lines.
- Default sidebar width increased to 300px; unified font sizes to even pixel values.
- Compact-mode timestamp/badge overlap resolved.

### Fixes

- **WS reconnect no longer clears the log area:** catchup now merges by lineNum (dedup) instead of overwriting the buffer on reconnect.
- **Tab lifecycle race guarded:** opening/closing tabs during async `file_tail` load can no longer create phantom WS subscriptions.
- **Bookmark line coordinates:** `file_tail` estimates line numbers; the WS `Subscribed` message now corrects them to exact `LineIndex::build` counts so bookmarks stay valid.
- **Sidebar search filter** now applies to both FILES and RECENT sections.
- **Statusbar** shows buffer cap (`entries.length / maxLines`) instead of drift-prone totalLines.
- **Hot-reload of active tab entries** after log-level config save.
- **Clipboard** extracted to `useClipboard` composable; removed HMR-breaking `destroy()` call.
- First/last tab ear clipping prevented via tabbar padding.
- Recent list no longer reorders/jumps on file click; tabbar hidden when no tabs; fixed bookmark panel height.
- File browser: removed file-dot, added empty-dir placeholder, restructured into sections.

### Refactor

- Merged `TabBar` into globalbar; unified height to `--tabbar-h`.
- Extracted `useTabs`, `useBookmarks`, `useRecentFiles`, `useHistoricalFilter`, `useClipboard` composables.
- Removed dead favorites feature code and dead `useLogStream.ts`.

### Protocol

- **`Subscribe`/`Unsubscribe` now use camelCase** (`afterSeq` instead of `after_seq`), matching the rest of the WS protocol and the documented convention.

## [v0.6.2] - 2026-06-30

### UI

- Search bar and log viewer style refinements

## [v0.6.1] - 2026-06-29

### Fixes

- WebSocket: detect dead connections via pong timeout (client) and idle timeout (server), fixing intermittent log tailing failures where connections silently died
- WebSocket: force reconnect on tab visibility regain and token change, preventing stale half-open connections
- FilterBar: search icon now renders above input background via z-index

### UI

- Increased small font sizes (9–13px → 10–14px) across components for readability
- Unified input/select backgrounds to `var(--bg)`
- Standardized control heights and centered settings rows

## [v0.6.0] - 2026-06-29

### Features

- Timestamp display: raw_timestamp field with Unix epoch support for accurate time rendering
- Display mode toggle: compact/cozy layouts with timestamp-first column order (LEVEL → TIME → message)
- Search history: suggestions dropdown in FilterBar, persisted to localStorage
- Chip editing: double-click to edit existing keyword chips
- Chip keyboard editing: Backspace on empty input reverts last chip for re-editing
- Timestamp column follows level color scheme (matches badge color per entry)
- Optimized line counting with memchr

### Fixes

- Level filter: all-levels-selected now equals no-filter, matching the initial default state (previously hid unclassified lines)
- WebSocket authentication: allow token via query parameter for browser compatibility
- FilterBar suggestions dropdown clipping fix (moved to filter-wrap container)
- Removed inaccurate line number column (replaced by level/time/message display order)

---

## [v0.5.1] - 2026-06-23

### Migration

- Repository moved from `wunamesst/tailr` to `flolibio/tailr`
- Updated all GitHub URLs in code, docs, and frontend
- Updated self-upgrade endpoint to new repository

---

## [v0.5.0] - 2026-06-23

### Security

- Path traversal protection: all file endpoints validate paths against configured `log_dirs` and `log_files` via `canonicalize()` + allowlist check
- Token authentication: optional Bearer token via `config.toml`, `TAILR_TOKEN` env var, or Settings UI
- CSRF protection: restricted CORS headers + `X-Requested-With` check on POST endpoints
- Config write protection: `POST /api/config/log-levels` requires authentication when token is set
- Error sanitization: generic error messages to client, detailed errors logged server-side
- Search parameter limits: `context` capped at 50, `limit` capped at 10000

### Features

- Token input dialog: auto-popup on 401, auto-reload file list after authentication
- Token setting in Settings dialog (persisted to localStorage)

---

## [v0.4.0] - 2026-06-22

### Features

- Settings dialog: modal design (VS Code style) replacing sidebar panel, with left navigation (General, Log Levels, About)
- Configurable log levels: 7 presets (General, Java, Python, PHP, Go, Rust, syslog), web UI for editing levels/keywords/colors with drag-and-drop reorder
- Font customization: font family dropdown (JetBrains Mono, Hack, Cascadia Code, Fira Code, Consolas, Monaco, Menlo, System Monospace) and font size (10–24px)
- Theme modes: Light, Dark, and System (follows OS preference), persisted across sessions
- Page title dynamically shows selected log file name (`tailr - <filename>`)
- About page with project logo, version, and GitHub link
- Warp-inspired dark theme color palette
- Full i18n support for all settings strings (en-US, zh-CN)

### Fixes

- Selection toolbar scoped to log viewer area only (no longer appears on other UI elements)
- Font size input uses `@change` to avoid fighting user keystrokes during typing
- Theme mode persisted to localStorage, restored on dialog reopen
- `setTimeout` timers properly cleared on component unmount

### Infrastructure

- `frontend/dist` removed from git tracking, added to `.gitignore`

---

## [v0.3.1] - 2026-06-15

### Performance

- Reverse-read tail: `LineIndex::tail_start()` reads backwards from EOF in 8KB chunks instead of scanning entire file (440MB log: 1.4s → 43ms)
- Wrap blocking tail I/O in `spawn_blocking` to avoid async executor stalls

### Fixes

- Dark mode text selection contrast (yellow → blue)
- Clippy `derivable_impls` lint: derive `Default` for `DaemonConfig`

### Infrastructure

- CONTRIBUTING.md, PR/Issue templates, CI workflow

---

## [v0.3.0] - 2026-06-14

### Features

- Config file support (`~/.config/tailr/config.toml`) with figment-based TOML/env/CLI merging
- CLI refactored from boolean flags to subcommands (`init`, `config`, `stop`, `status`, `systemd`, `launchd`, `upgrade`)

### Fixes

- Self-upgrade now bypasses `bump_is_compatible` to always reach latest version
- Sidebar overlapping statusbar in grid layout
- Default config template: `log` and `bind` uncommented

---

## [v0.2.0] - 2026-06-12

### Features

- Resizable sidebar with drag handle (180–400px range)
- Selection toolbar with copy and follow-keyword actions

### Fixes

- Copy feedback simplified to icon-only toggle
- Selection toolbar shows only after mouse release
- Sidebar resize handle bounds and hit area

---

## [v0.1.5] - 2026-06-10

### Fixes

- File truncation detection in LineIndex (file size shrink check)

---

## [v0.1.4] - 2026-06-10

### Features

- Daemon mode with background process management (`daemonize`)
- Settings footer with version and GitHub link

### Fixes

- Daemon mode HTTP failure (fork before tokio runtime)
- Settings footer hidden behind status bar
- Regex filter test correctness

---

## [v0.1.3] - 2026-06-08

### Features

- Self-upgrade (`tailr upgrade`)
- Internationalization (en-US, zh-CN)

### Fixes

- CLI version comparison and `--check` flag
- musl cross-compilation (switch to rustls for self_update)

### Infrastructure

- Release artifacts packaged as tar.gz with unified binary name

---

## [v0.1.2] - 2026-06-06

### Infrastructure

- CI release workflow
- Logo

---

## [v0.1.1] - 2026-06-05

### Features

- Initial release: log tail/search server with WebSocket streaming, multi-keyword filter, log level detection, web UI

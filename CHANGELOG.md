# Changelog

## [v0.10.0] - 2026-07-22

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

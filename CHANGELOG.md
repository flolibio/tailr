# Changelog

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

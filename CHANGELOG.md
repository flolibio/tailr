# Changelog

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

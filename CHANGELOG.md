# Changelog

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

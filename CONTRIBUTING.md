# Contributing to tailr

Thanks for your interest in contributing! This guide covers everything you need to get started.

## Prerequisites

- **Rust** 1.70+ (`rustup show`)
- **Node.js** 20+ (check `.nvmrc`; use `nvm use`)
- **npm** 10+
- **Docker** (only for Linux cross-compilation)

## Getting Started

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/tailr.git
cd tailr

# Add upstream
git remote add upstream https://github.com/flolibio/tailr.git

# Build frontend (required before cargo build)
make frontend

# Start backend
cargo run

# In another terminal, start Vite dev server
cd frontend && npm run dev
```

The Vite dev server runs on `:5173` and proxies `/api` and `/ws` to `:7700`.

## Development Workflow

1. **Create a branch** from `main`:
   ```bash
   git checkout -b fix/short-description
   ```

2. **Make changes**, test locally.

3. **Run checks before committing**:
   ```bash
   make test    # cargo test + clippy + vue-tsc
   # or individually:
   make test-backend    # cargo test && cargo clippy -- -D warnings
   make test-frontend   # vue-tsc --noEmit
   ```

4. **Commit** using [Conventional Commits](https://www.conventionalcommits.org/):
   ```
   feat: add new search filter
   fix: correct file truncation detection
   docs: update API documentation
   refactor: simplify config loading
   chore: bump dependencies
   ```

5. **Push and open a PR**:
   ```bash
   git push origin fix/short-description
   ```
   Then open a Pull Request on GitHub targeting `main`.

## Branch Naming

| Type | Format | Example |
|------|--------|---------|
| Bug fix | `fix/description` | `fix/truncation-detection` |
| Feature | `feat/description` | `feat/regex-search` |
| Refactor | `refactor/description` | `refactor/cli-subcommands` |
| Docs | `docs/description` | `docs/api-reference` |

## Code Style

### Rust

- Follow `rustfmt` defaults (no custom config).
- Run `cargo clippy -- -D warnings` — zero warnings allowed.
- No `unwrap()` in production code paths (use `?` or `expect()` with context).
- No `as any` / type suppression.
- JSON field casing: `camelCase` via `serde(rename_all)`.
- WS protocol: tagged enum via `serde(tag = "type", rename_all = "camelCase")`.

### Frontend (Vue 3 + TypeScript)

- Use `<script setup>` composition API.
- No `any` types — define proper interfaces.
- Run `vue-tsc --noEmit` before committing.
- Follow existing CSS variable naming (`--c-*`, `--sidebar-*`, etc.).

### Frontend Build

Frontend dist is **gitignored** and built on demand. It is embedded into the binary via `include_dir!` at compile time.

Always use `npm ci` (not `npm install`) to avoid modifying `package-lock.json`:

```bash
cd frontend && npm ci && npm run build
```

Or simply:

```bash
make frontend
```

If `package-lock.json` appears modified after `npm install`, do **not** commit it. Run `npm ci` to restore the correct state, or `git checkout -- package-lock.json`.

Only commit `package-lock.json` changes when you intentionally add or upgrade dependencies.

## Testing

```bash
# All tests
cargo test --workspace

# Single crate
cargo test -p tailr-tail-engine

# Single test
cargo test -p tailr-search-engine test_literal_search_basic
```

Tests use `tempfile::NamedTempFile` — no external services required.

## Project Structure

```
src/
  main.rs           # CLI (clap), subcommand dispatch
  config.rs         # Config file loading (figment)
  daemon.rs         # Daemonization, PID, signal handling
crates/
  protocol/         # Shared types: LogEntry, WSMessage, LogLevel
  tail-engine/      # File watching (notify), LineIndex (mmap)
  search-engine/    # Grep search (grep-regex/grep-searcher)
  server/           # Axum app: REST API, WebSocket, static files
frontend/           # Vue 3 + TypeScript + Vite SPA
```

## Release Process

Releases are automated via GitHub Actions. Maintainers only need to push tags:

```bash
git tag -a vX.Y.Z -m "vX.Y.Z: description"
git push origin vX.Y.Z
```

CI builds binaries and creates a **draft release**. Publish it manually after verification.

**SemVer rules**:
- PATCH (0.0.x): Bug fixes
- MINOR (0.x.0): New features, backward-compatible changes
- MAJOR (x.0.0): Breaking changes

## Questions?

- Open an [issue](https://github.com/flolibio/tailr/issues) for bugs or feature requests.
- Start a [discussion](https://github.com/flolibio/tailr/discussions) for questions.

# Contributing to tailr

Thanks for your interest in contributing! 🎉

## Quick Start

```bash
# Clone
git clone https://github.com/flolibio/tailr.git
cd tailr

# Build frontend
make frontend

# Run dev server
cargo run
```

## Development Workflow

### Prerequisites

- Rust 1.75+
- Node.js 18+
- npm

### Build Commands

```bash
make frontend          # Build frontend (npm ci + npm run build)
make build             # Build everything (frontend + cargo build --release)
make dev               # Run cargo (run `make frontend` first)
make check             # Type check (cargo check)
make test              # Run all tests (cargo test + clippy + vue-tsc)
```

### Running Locally

```bash
# Terminal 1: Rust backend
cargo run              # Starts on 0.0.0.0:7700

# Terminal 2: Vite dev server (with proxy)
cd frontend && npm run dev   # Starts on :5173, proxies to :7700
```

## Code Structure

```
src/main.rs           # CLI entrypoint
src/config.rs         # Config loading (figment)
src/daemon.rs         # Daemonization, PID file, signals
crates/
  protocol/           # Shared types (LogEntry, WSMessage, LogLevel)
  tail-engine/        # File watching (notify), LineIndex (memmap2)
  search-engine/      # Grep-based search, LogFilter
  server/             # Axum app: REST API, WebSocket, static files
frontend/             # Vue 3 + TypeScript + Vite SPA
```

## Guidelines

### Code Style

- **Rust**: Default rustfmt, follow clippy warnings
- **TypeScript**: Vue 3 Composition API, TypeScript strict mode
- **JSON**: camelCase everywhere (serde `rename_all`)

### Commit Messages

```
type(scope): description

# Examples
feat(search): add time range filter
fix(ws): handle connection drop gracefully
docs(readme): update installation steps
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

### Pull Requests

1. Fork the repo
2. Create a feature branch (`git checkout -b feat/amazing-feature`)
3. Commit your changes
4. Push to your fork
5. Open a PR against `main`

PR checklist:
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cd frontend && npx vue-tsc --noEmit` passes
- [ ] Manually tested the changes

## Reporting Issues

Use the [Bug Report](https://github.com/flolibio/tailr/issues/new?template=bug_report.md) template.

Include:
- OS and version
- tailr version (`tailr --version`)
- Steps to reproduce
- Expected vs actual behavior

## Feature Requests

Use the [Feature Request](https://github.com/flolibio/tailr/issues/new?template=feature_request.md) template.

## Questions?

Open a [Discussion](https://github.com/flolibio/tailr/discussions) or check existing issues.

---

**Thank you for contributing!** 🚀

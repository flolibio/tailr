.PHONY: build build-linux build-linux-arm dev debug check test test-backend test-frontend clean release release-linux release-linux-arm frontend

# ── Development ──

LOG ?= /var/log

dev:
	cargo run

debug: frontend
	cargo run -- --log $(LOG)

# ── Code Check ──

check:
	cargo check

test: test-backend test-frontend

test-backend:
	cargo test
	cargo clippy -- -D warnings

test-frontend:
	cd frontend && npx vue-tsc --noEmit

# ── Frontend ──

frontend:
	cd frontend && npm install && npm run build

# ── Local Build ──

build: frontend
	cargo build --release

# ── Linux Build (Docker) ──

build-linux:
	docker run --rm --platform linux/amd64 -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add x86_64-unknown-linux-musl && cargo build --release --target x86_64-unknown-linux-musl"

build-linux-arm:
	docker run --rm -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add aarch64-unknown-linux-musl && cargo build --release --target aarch64-unknown-linux-musl"

# ── Release (frontend + all platforms) ──

release: frontend release-linux release-linux-arm
	@echo ""
	@echo "✓ Release artifacts:"
	@echo "  Linux x86_64: dist/tailr-x86_64-linux-musl.tar.gz"
	@echo "  Linux ARM64:  dist/tailr-aarch64-linux-musl.tar.gz"

release-linux: frontend
	docker run --rm --platform linux/amd64 -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add x86_64-unknown-linux-musl && cargo build --release --target x86_64-unknown-linux-musl"
	mkdir -p dist
	tar czf dist/tailr-x86_64-linux-musl.tar.gz -C target/x86_64-unknown-linux-musl/release tailr

release-linux-arm: frontend
	docker run --rm -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add aarch64-unknown-linux-musl && cargo build --release --target aarch64-unknown-linux-musl"
	mkdir -p dist
	tar czf dist/tailr-aarch64-linux-musl.tar.gz -C target/aarch64-unknown-linux-musl/release tailr

# ── Clean ──

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules dist

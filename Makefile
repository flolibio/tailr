.PHONY: build build-linux build-linux-arm dev clean check release release-linux release-linux-arm frontend

# ── Development ──

dev:
	cargo run

check:
	cargo check

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
	@echo "  Linux x86_64: target/x86_64-unknown-linux-musl/release/tailr"
	@echo "  Linux ARM64:  target/aarch64-unknown-linux-musl/release/tailr"

release-linux: frontend
	docker run --rm --platform linux/amd64 -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add x86_64-unknown-linux-musl && cargo build --release --target x86_64-unknown-linux-musl"

release-linux-arm: frontend
	docker run --rm -v "$(CURDIR)":/app -w /app rust:1.94 \
		sh -c "rustup target add aarch64-unknown-linux-musl && cargo build --release --target aarch64-unknown-linux-musl"

# ── Clean ──

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules

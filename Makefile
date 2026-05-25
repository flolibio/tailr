.PHONY: build dev clean check

build:
	cargo build --release

dev:
	cargo run

clean:
	cargo clean

check:
	cargo check

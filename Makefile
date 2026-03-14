.PHONY: fmt lint test check audit build run seed

# Development
fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

test:
	cargo test

# CI: runs everything that the GitHub Actions workflow checks
check: fmt-check lint test

fmt-check:
	cargo fmt -- --check

# Security
audit:
	cargo audit

# Build & Run
build:
	cargo build --release

run:
	cargo run -- serve

seed:
	cargo run -- seed --app leanfin
	cargo run -- seed --app mindflow

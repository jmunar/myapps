# Use bash so version-bump targets can rely on `<<<` herestrings.
SHELL := /bin/bash

.PHONY: fmt lint test check audit upgrade build run screenshots gh-env bump-patch bump-minor bump-major

# Development
fmt:
	cargo fmt

lint:
	cargo clippy --workspace -- -D warnings

test:
	cargo test --workspace

# CI: runs everything that the GitHub Actions workflow checks
check: fmt-check lint test

fmt-check:
	cargo fmt -- --check

# Dependencies
upgrade:
	cargo update
	cargo audit

# Security
audit:
	cargo audit

# Version bumping — reads version from [package] section of Cargo.toml
define get_version
$$(grep -A2 '^\[package\]' Cargo.toml | grep '^version' | cut -d'"' -f2)
endef

bump-patch:
	@VERSION=$(get_version); \
	IFS='.' read -r MAJOR MINOR PATCH <<< "$$VERSION"; \
	PATCH=$$((PATCH + 1)); \
	NEW="$$MAJOR.$$MINOR.$$PATCH"; \
	sed 's/^version = "'"$$VERSION"'"/version = "'"$$NEW"'"/' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	echo "$$VERSION → $$NEW"

bump-minor:
	@VERSION=$(get_version); \
	IFS='.' read -r MAJOR MINOR PATCH <<< "$$VERSION"; \
	MINOR=$$((MINOR + 1)); PATCH=0; \
	NEW="$$MAJOR.$$MINOR.$$PATCH"; \
	sed 's/^version = "'"$$VERSION"'"/version = "'"$$NEW"'"/' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	echo "$$VERSION → $$NEW"

bump-major:
	@VERSION=$(get_version); \
	IFS='.' read -r MAJOR MINOR PATCH <<< "$$VERSION"; \
	MAJOR=$$((MAJOR + 1)); MINOR=0; PATCH=0; \
	NEW="$$MAJOR.$$MINOR.$$PATCH"; \
	sed 's/^version = "'"$$VERSION"'"/version = "'"$$NEW"'"/' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	echo "$$VERSION → $$NEW"

# Build & Run
build:
	cargo build --release

run:
	cargo run -- serve

# Screenshots for README (requires Node.js)
screenshots:
	./scripts/take-screenshots.sh

# CD: create GitHub environments and set variables from deploy/*.env
gh-env:
	@BUILD_DIR=""; \
	for f in deploy/*.env; do \
		val=$$(grep '^DEPLOY_REMOTE_BUILD_DIR=' "$$f" | head -1 | cut -d= -f2- | sed 's/^"//;s/"$$//'); \
		if [ -n "$$val" ]; then \
			if [ -z "$$BUILD_DIR" ]; then \
				BUILD_DIR="$$val"; \
			elif [ "$$BUILD_DIR" != "$$val" ]; then \
				echo "ERROR: DEPLOY_REMOTE_BUILD_DIR mismatch: '$$BUILD_DIR' vs '$$val' in $$f"; \
				exit 1; \
			fi; \
		fi; \
	done
	@for f in deploy/*.env; do \
		GH_ENV=$$(grep '^DEPLOY_GH_ENVIRONMENT=' "$$f" | head -1 | cut -d= -f2-); \
		if [ -z "$$GH_ENV" ]; then \
			echo "ERROR: DEPLOY_GH_ENVIRONMENT not set in $$f"; \
			exit 1; \
		fi; \
		echo "==> Creating environment: $$GH_ENV"; \
		gh api "repos/{owner}/{repo}/environments/$$GH_ENV" -X PUT --silent; \
		while IFS='=' read -r key value; do \
			case "$$key" in \
				''|\#*|DEPLOY_GH_ENVIRONMENT) continue ;; \
			esac; \
			value=$$(echo "$$value" | sed 's/^"//;s/"$$//'); \
			if [ -z "$$value" ]; then \
				echo "  Skipping $$key (empty)"; \
			else \
				echo "  Setting $$key=$$value"; \
				gh variable set "$$key" --env "$$GH_ENV" --body "$$value"; \
			fi; \
		done < "$$f"; \
	done

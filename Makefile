# Use bash so version-bump targets can rely on `<<<` herestrings.
SHELL := /bin/bash

.PHONY: fmt lint test check audit upgrade build build-arm64 package-arm64 deploy-stage deploy-prod run screenshots gh-env bump-patch bump-minor bump-major

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

# Cross-compile for the Odroid N2 (aarch64) using `cross` + Docker.
# The host's sccache binary is dynamically linked against the host's libssl, so
# we can't bind-mount it into the cross container. Instead, we download a
# musl-static sccache release (one-time, ~10MB) into ~/.cache/cross-tools and
# bind-mount that. Cache dir is ~/.cache/sccache-cross — kept separate from
# the native sccache cache to avoid root-owned files leaking into it.
SCCACHE_STATIC_VERSION := v0.15.0
SCCACHE_STATIC_DIR := $(HOME)/.cache/cross-tools
SCCACHE_STATIC_BIN := $(SCCACHE_STATIC_DIR)/sccache-$(SCCACHE_STATIC_VERSION)

$(SCCACHE_STATIC_BIN):
	@mkdir -p $(SCCACHE_STATIC_DIR)
	@echo "▸ Downloading static sccache $(SCCACHE_STATIC_VERSION) for cross builds..."
	@curl -sSfL "https://github.com/mozilla/sccache/releases/download/$(SCCACHE_STATIC_VERSION)/sccache-$(SCCACHE_STATIC_VERSION)-x86_64-unknown-linux-musl.tar.gz" \
		| tar -xz --strip-components=1 -C $(SCCACHE_STATIC_DIR) "sccache-$(SCCACHE_STATIC_VERSION)-x86_64-unknown-linux-musl/sccache"
	@mv $(SCCACHE_STATIC_DIR)/sccache $@
	@chmod +x $@

build-arm64: $(SCCACHE_STATIC_BIN)
	@command -v cross >/dev/null  || { echo "cross not found: cargo install cross --git https://github.com/cross-rs/cross"; exit 1; }
	@command -v docker >/dev/null || { echo "docker not found"; exit 1; }
	@mkdir -p $$HOME/.cache/sccache-cross
	@unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER; \
	export CROSS_CONTAINER_OPTS="-e RUSTC_WRAPPER=sccache -e SCCACHE_DIR=/sccache -v $(SCCACHE_STATIC_BIN):/usr/local/bin/sccache:ro -v $$HOME/.cache/sccache-cross:/sccache"; \
	cross build --release --target aarch64-unknown-linux-gnu --features vendored-openssl

# Assemble a deployable bundle (binary + static/) for ./deploy.sh release-deploy.
RELEASE_PKG_DIR := target/aarch64-unknown-linux-gnu/release-pkg

package-arm64: build-arm64
	@rm -rf $(RELEASE_PKG_DIR)
	@mkdir -p $(RELEASE_PKG_DIR)
	cp target/aarch64-unknown-linux-gnu/release/myapps $(RELEASE_PKG_DIR)/
	cp -r static $(RELEASE_PKG_DIR)/
	@echo "▸ Release bundle ready: $(RELEASE_PKG_DIR)"

# Cross-build + package + push to staging via the existing release-deploy path.
deploy-stage: package-arm64
	./deploy.sh stage release-deploy $(RELEASE_PKG_DIR)

# Same for production. Normally prod ships via CI; use this only for hotfixes.
deploy-prod: package-arm64
	./deploy.sh prod release-deploy $(RELEASE_PKG_DIR)

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

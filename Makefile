.PHONY: fmt lint test check audit upgrade build run screenshots gh-env

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
		GH_ENV=""; \
		while IFS='=' read -r key value; do \
			case "$$key" in \
				''|\#*) continue ;; \
			esac; \
			value=$$(echo "$$value" | sed 's/^"//;s/"$$//'); \
			if [ "$$key" = "DEPLOY_GH_ENVIRONMENT" ]; then \
				GH_ENV="$$value"; \
				echo "==> Creating environment: $$GH_ENV"; \
				gh api "repos/{owner}/{repo}/environments/$$GH_ENV" -X PUT --silent; \
			else \
				if [ -z "$$value" ]; then \
					echo "  Skipping $$key (empty)"; \
				else \
					echo "  Setting $$key=$$value"; \
					gh variable set "$$key" --env "$$GH_ENV" --body "$$value"; \
				fi; \
			fi; \
		done < "$$f"; \
	done

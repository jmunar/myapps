.PHONY: fmt lint test check audit build run seed gh-env

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

# CD: create GitHub environments and set variables from deploy/*.env
gh-env:
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

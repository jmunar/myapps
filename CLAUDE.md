# MyApps

Multi-app personal platform. LeanFin (personal expense management), MindFlow
(thought capture & mind map), and VoiceToText (audio transcription) are the
current sub-applications. After login, users
see an app launcher and can navigate into individual apps. All apps share auth,
DB, layout/styling, and config.

## Stack

- **Backend**: Rust, Axum, SQLite (sqlx)
- **Frontend**: HTMX + server-rendered HTML
- **Deploy target**: Odroid N2 (aarch64, Ubuntu Server 24.04), behind nginx

## Build & Run

```bash
# Development (local)
cargo run -- serve                  # Start HTTP server on 127.0.0.1:3000
cargo run -- sync                   # Run transaction sync manually
cargo run -- create-user            # Create a user
cargo run -- seed --app leanfin     # Seed LeanFin demo data
cargo run -- seed --app leanfin --reset  # Wipe and re-seed demo data
cargo run -- seed --app mindflow   # Seed MindFlow demo data
cargo run -- seed --app mindflow --reset  # Wipe and re-seed demo data

# Makefile shortcuts
make check                          # fmt-check + clippy + test (same as CI)
make fmt                            # Auto-format code
make lint                           # Run clippy with -D warnings
make test                           # Run all tests
make audit                          # Security audit (cargo audit)
make build                          # Release build
make run                            # Start dev server

# Deploy to server (rsyncs source, builds on Odroid, installs + restarts)
export MYAPPS_SERVER="user@odroid.local"
./deploy.sh setup               # First time only
./deploy.sh deploy              # Build + install + restart
```

## CI/CD

- **GitHub Actions CI** (`.github/workflows/ci.yml`) runs on every push to
  `main` and on PRs: format check, clippy (warnings-as-errors), and tests.
- **Security audit** (`.github/workflows/audit.yml`) runs on Cargo.toml/lock
  changes and weekly via `cargo audit`.
- **Dependabot** (`.github/dependabot.yml`) opens weekly PRs for Cargo
  dependency updates and GitHub Actions version bumps.
- `make check` runs the same checks locally before pushing.

## Project Conventions

- SQL queries use runtime-checked sqlx (no compile-time macros).
- Migrations live in `migrations/` and run automatically on startup.
- Environment variables are loaded from `.env` in development (via dotenvy).
- No secrets in the repo. See `.env.example` for required variables.
- Keep memory footprint minimal — avoid unnecessary allocations and large
  dependencies.
- LeanFin-specific routes, handlers, and services live under `src/apps/leanfin/`.
- MindFlow-specific routes, handlers, and services live under `src/apps/mindflow/`.
- VoiceToText-specific routes, handlers, and services live under `src/apps/voice_to_text/`.
- Shared infrastructure (auth, config, db, models, layout) stays at the top level.
- All app-specific database tables use the app name as prefix (e.g. `leanfin_accounts`, `mindflow_thoughts`, `voice_jobs`).

## Testing

- After any frontend change (routes, handlers, HTML templates, CSS classes used
  in assertions), run the **frontend-tester agent**
  (`.claude/agents/frontend-tester.md`) to generate or update integration tests.
- Tests live in `tests/` and use `axum-test`; see the agent file for patterns.

## Documentation

- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Deployment](docs/deployment.md)
- [Worktree Workflow](docs/worktree-workflow.md)

# MyApps

Multi-app personal platform. LeanFin (personal expense management), MindFlow
(thought capture & mind map), VoiceToText (audio transcription), and
ClassroomInput (classroom marks & notes recording) are the current
sub-applications. After login, users
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
cargo run -- cron                   # Run scheduled app tasks (e.g. bank sync)
cargo run -- create-user            # Create a user (admin, direct password)
cargo run -- invite                 # Generate a single-use invite link (48h)
cargo run -- seed --user <name>              # Seed all apps for a user
cargo run -- delete-user --username <name>      # Delete a user and all their data
cargo run -- delete-user-app-data --username <name>          # Delete all app data (keeps user)
cargo run -- delete-user-app-data --username <name> --app X  # Delete data for one app
cargo run -- cleanup-users --days 7             # Delete users inactive >7 days

# Makefile shortcuts
make check                          # fmt-check + clippy + test (same as CI)
make fmt                            # Auto-format code
make lint                           # Run clippy with -D warnings
make test                           # Run all tests
make audit                          # Security audit (cargo audit)
make build                          # Release build
make run                            # Start dev server

# Deploy to server (rsyncs source via deploy user, builds on Odroid, installs + restarts)
./deploy.sh prod setup                    # First time only
./deploy.sh prod deploy                   # Build + install + restart
./deploy.sh stage setup                   # First time only (staging)
./deploy.sh stage deploy                  # Build + install + restart (staging)
./deploy.sh stage deploy                  # Deploy (auto-seeds on invite registration)
```

## CI/CD

- **GitHub Actions CI** (`.github/workflows/ci.yml`) runs on every push to
  `main` and on PRs: format check, clippy (warnings-as-errors), and tests.
- **GitHub Actions CD** (`.github/workflows/cd.yml`) runs on every push to
  `main`: deploys to staging (with smoke test), then to production (with smoke
  test). Uses `DEPLOY_CI=true` for non-interactive SSH. Requires GitHub
  Environments (`staging`, `production`) with deploy config variables and SSH
  secrets. See `docs/deployment.md` for setup details.
- **Security audit** (`.github/workflows/audit.yml`) runs on Cargo.toml/lock
  changes and weekly via `cargo audit`.
- All three workflows support `workflow_dispatch` for manual triggering from
  the GitHub Actions UI.
- **Dependabot** (`.github/dependabot.yml`) opens weekly PRs for Cargo
  dependency updates and GitHub Actions version bumps.
- `make check` runs the same checks locally before pushing.

## Project Conventions

- SQL queries use runtime-checked sqlx (no compile-time macros).
- Core migrations (auth, sessions, settings) live in `migrations/`.
  App-specific migrations live in each app's `migrations/` directory
  (e.g. `src/apps/leanfin/migrations/`). All are merged by timestamp
  and run automatically on startup via `db::migrator()`.
- Environment variables are loaded from `.env` in development (via dotenvy).
- No secrets in the repo. See `.env.example` for required variables.
- Keep memory footprint minimal — avoid unnecessary allocations and large
  dependencies.
- LeanFin-specific routes, handlers, and services live under `src/apps/leanfin/`.
- MindFlow-specific routes, handlers, and services live under `src/apps/mindflow/`.
- VoiceToText-specific routes, handlers, and services live under `src/apps/voice_to_text/`.
- ClassroomInput-specific routes and handlers live under `src/apps/classroom_input/`.
- Shared infrastructure (auth, config, db, models, layout, i18n, command,
  services) stays at the top level. Shared services (whisper transcription,
  push notifications) live in `src/services/`.
- Each app implements the `App` trait in `src/apps/registry.rs`. The trait
  provides hooks for migrations, routing, commands, seeding, scheduled tasks
  (`cron`), and background workers (`on_serve`). Adding a new app means
  implementing the trait and registering in `all_app_instances()`.
- The command bar module (`src/command/`) handles LLM-powered natural-language
  command interpretation and execution via a llama.cpp server.
- Each app exposes an `ops.rs` module with shared action functions callable from
  both HTTP handlers and the command bar dispatcher. New actions go in `ops.rs`.
- Shared translations (auth, launcher, command bar) live in `src/i18n/`.
  App-specific translations live in each app's `i18n.rs` module. Both use
  compile-time struct-based translations; adding a field forces both EN and ES
  to be updated.
- All app-specific database tables use the app name as prefix (e.g. `leanfin_accounts`, `mindflow_thoughts`, `voice_to_text_jobs`, `classroom_input_classrooms`).
- When adding or removing environment variables, update all four places:
  `.env.example`, `deploy/*.env.example`, the `.env` template in `deploy.sh`
  (`setup()`), and the Environment Variables section in `docs/deployment.md`.

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

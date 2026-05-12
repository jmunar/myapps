# MyApps

Multi-app personal platform. LeanFin (personal expense management), MindFlow
(thought capture & mind map), VoiceToText (audio transcription),
FormInput (custom forms with row sets and column-typed inputs), and Notes
(markdown-based note-taking) are the current sub-applications. After login, users
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
make screenshots                    # Regenerate README screenshots (needs Node.js)

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
  `main`: reads the version from `Cargo.toml`, creates a git tag and GitHub
  Release with a cross-compiled aarch64 tarball containing the binary and
  static assets (using `cross`), then deploys
  to staging and production (with smoke tests). Version is bumped during
  development via `make bump-{patch,minor,major}` (automated in
  `/finish-development`). Uses `DEPLOY_CI=true` for non-interactive SSH.
  Requires GitHub Environments (`staging`, `production`) with deploy config
  variables and SSH secrets. See `docs/deployment.md` for setup details.
- **Security audit** (`.github/workflows/audit.yml`) runs on Cargo.toml/lock
  changes and weekly via `cargo audit`.
- All three workflows support `workflow_dispatch` for manual triggering from
  the GitHub Actions UI. Manual CD runs deploy to staging only by default;
  tick the `deploy_prod` input to also deploy to production.
- **Dependabot** (`.github/dependabot.yml`) opens weekly PRs for Cargo
  dependency updates and GitHub Actions version bumps.
- `make check` runs the same checks locally before pushing.

## Workspace Structure

The project is a Cargo workspace with separate crates:

```
crates/
  myapps-core/           # Shared infra: auth, config, db, i18n, layout, routes, services, command, registry
  myapps-leanfin/        # LeanFin app
  myapps-mindflow/       # MindFlow app
  myapps-voice-to-text/  # VoiceToText app
  myapps-form-input/      # FormInput app
  myapps-notes/           # Notes app
src/
  main.rs                # Thin binary: CLI + app registration
  lib.rs                 # Re-export facade for tests
```

Apps depend on `myapps-core`. No app depends on another app. The root binary
assembles all crates.

## Project Conventions

- SQL queries use runtime-checked sqlx (no compile-time macros).
- Core migrations live in `crates/myapps-core/migrations/`.
  App-specific migrations live in each app crate's `migrations/` directory
  (e.g. `crates/myapps-leanfin/migrations/`). All are merged by timestamp
  and run automatically on startup via `db::migrator()`.
- Environment variables are loaded from `.env` in development (via dotenvy).
- No secrets in the repo. See `.env.example` for required variables.
- Keep memory footprint minimal — avoid unnecessary allocations and large
  dependencies.
- LeanFin-specific routes, handlers, and services live in `crates/myapps-leanfin/`.
- MindFlow-specific routes, handlers, and services live in `crates/myapps-mindflow/`.
- VoiceToText-specific routes, handlers, and services live in `crates/myapps-voice-to-text/`.
- FormInput-specific routes and handlers live in `crates/myapps-form-input/`.
- Notes-specific routes and handlers live in `crates/myapps-notes/`.
- Shared infrastructure (auth, config, db, models, layout, i18n, command,
  components, services) lives in `crates/myapps-core/`. Shared services (whisper
  transcription, push notifications) live in `crates/myapps-core/src/services/`.
- Each app implements the `App` trait from `myapps_core::registry`. The trait
  provides hooks for migrations, routing, CSS, commands, seeding, scheduled
  tasks (`cron`), and background workers (`on_serve`). To add a new app, run
  `/add-app <AppName>` which scaffolds the crate and wires it into the
  workspace. External app shortcuts (services outside MyApps) are configured
  via the `EXTERNAL_APPS` env var, not the `App` trait.
- The command bar module (`crates/myapps-core/src/command/`) handles LLM-powered
  natural-language command interpretation and execution via a llama.cpp server.
- Each app exposes an `ops.rs` module with shared action functions callable from
  both HTTP handlers and the command bar dispatcher. New actions go in `ops.rs`.
- Shared translations (auth, launcher, command bar) live in
  `crates/myapps-core/src/i18n/`. App-specific translations live in each app
  crate's `i18n.rs` module. Both use compile-time struct-based translations;
  adding a field forces both EN and ES to be updated.
- All app-specific database tables use the app name as prefix (e.g. `leanfin_accounts`, `mindflow_thoughts`, `voice_to_text_jobs`, `form_input_row_sets`, `notes_notes`, `notes_note_updates`).
- When adding or removing environment variables, update all four places:
  `.env.example`, `deploy/*.env.example`, the `.env` template in `deploy.sh`
  (`setup()`), and the Environment Variables section in `docs/deployment.md`.

## Testing

- After any frontend change (routes, handlers, HTML templates, CSS classes used
  in assertions), run the **frontend-tester agent**
  (`.claude/agents/frontend-tester.md`) to generate or update integration tests.
- For browser-level verification (XSS, broken HTMX swaps, console errors,
  4xx/5xx, layout regressions) use the **`/frontend-walkthrough`** command.
  No-arg form walks every route touched on the current branch vs `main`;
  pass an app key, route, or description to walk a specific area on demand.
  See `.claude/commands/frontend-walkthrough.md`.
- App-specific tests live in each app crate's `tests/` directory
  (e.g. `crates/myapps-leanfin/tests/`). Platform-level auth and launcher
  tests live at the root `tests/`. The shared `myapps-test-harness` crate
  (`crates/myapps-test-harness/`) provides `spawn_app()` and `TestApp`
  helpers. Tests use `axum-test`; see the agent file for patterns.

## Documentation

- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Deployment](docs/deployment.md)
- [Worktree Workflow](docs/worktree-workflow.md)

# MyApps

Multi-app personal platform. LeanFin (personal expense management) is the first
sub-application. After login, users see an app launcher and can navigate into
individual apps. All apps share auth, DB, layout/styling, and config.

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

# Tests
cargo test

# Deploy to server (rsyncs source, builds on Odroid, installs + restarts)
export MYAPPS_SERVER="user@odroid.local"
./deploy.sh setup               # First time only
./deploy.sh deploy              # Build + install + restart
```

## Project Conventions

- SQL queries use runtime-checked sqlx (no compile-time macros).
- Migrations live in `migrations/` and run automatically on startup.
- Environment variables are loaded from `.env` in development (via dotenvy).
- No secrets in the repo. See `.env.example` for required variables.
- Keep memory footprint minimal — avoid unnecessary allocations and large
  dependencies.
- LeanFin-specific routes, handlers, and services live under `src/apps/leanfin/`.
- Shared infrastructure (auth, config, db, models, layout) stays at the top level.

## Documentation

- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Deployment](docs/deployment.md)
- [Worktree Workflow](docs/worktree-workflow.md)

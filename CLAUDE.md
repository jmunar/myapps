# LeanFin

Personal expense management app that fetches bank transactions via Enable Banking
(PSD2) and lets you label/categorize them.

## Stack

- **Backend**: Rust, Axum, SQLite (sqlx), Askama templates
- **Frontend**: HTMX + server-rendered HTML
- **Deploy target**: Odroid N2 (aarch64, Ubuntu Server 24.04), behind nginx

## Build & Run

```bash
# Development (local)
cargo run -- serve              # Start HTTP server on 127.0.0.1:3000
cargo run -- sync               # Run transaction sync manually
cargo run -- create-user        # Create a user

# Tests
cargo test

# Deploy to server (rsyncs source, builds on Odroid, installs + restarts)
export LEANFIN_SERVER="user@odroid.local"
./deploy.sh setup               # First time only
./deploy.sh deploy              # Build + install + restart
```

## Project Conventions

- All SQL queries are compile-time checked via sqlx. Run `cargo sqlx prepare`
  before committing if queries change.
- Migrations live in `migrations/` and run automatically on startup.
- Environment variables are loaded from `.env` in development (via dotenvy).
- No secrets in the repo. See `.env.example` for required variables.
- Keep memory footprint minimal — avoid unnecessary allocations and large
  dependencies.

## Documentation

- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Deployment](docs/deployment.md)

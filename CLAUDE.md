# LeanFin

Personal expense management app that fetches bank transactions via Enable Banking
(PSD2) and lets you label/categorize them.

## Stack

- **Backend**: Rust, Axum, SQLite (sqlx), Askama templates
- **Frontend**: HTMX + server-rendered HTML
- **Deploy target**: Raspberry Pi (aarch64), behind nginx + certbot

## Build & Run

```bash
# Development
cargo run -- serve              # Start HTTP server on 127.0.0.1:3000
cargo run -- sync               # Run transaction sync manually
cargo run -- create-user        # Create a user

# Tests
cargo test

# Cross-compile for Raspberry Pi
cross build --release --target aarch64-unknown-linux-gnu

# Deploy
./deploy.sh
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

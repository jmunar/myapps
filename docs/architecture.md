# LeanFin — Architecture

## Tech Stack

| Layer            | Choice                          |
|------------------|---------------------------------|
| Language         | Rust                            |
| HTTP framework   | Axum                            |
| Database         | SQLite (via sqlx, compile-time checked queries) |
| Templating       | Askama (compile-time HTML templates) |
| Frontend         | HTMX + minimal CSS (Pico CSS or similar) |
| Auth             | Argon2 + server-side sessions   |
| Bank aggregator  | Enable Banking PSD2 API         |
| Notifications    | Telegram Bot API                |
| Reverse proxy    | nginx + certbot (pre-existing)  |
| Process manager  | systemd                         |

## Binary Structure

A single binary with subcommands:

```
leanfin serve          # Start the HTTP server
leanfin sync           # Fetch transactions from all linked accounts (cron)
leanfin create-user    # Create a user from the command line
```

All subcommands share the same configuration and database.

## Project Layout

```
leanfin/
├── docs/                    # Documentation
├── migrations/              # SQLite migrations (sqlx)
├── src/
│   ├── main.rs              # CLI entrypoint (clap subcommands)
│   ├── config.rs            # Configuration (env vars / config file)
│   ├── db.rs                # Database pool and migrations
│   ├── models/              # Domain types (Transaction, Account, Label, etc.)
│   ├── routes/              # Axum route handlers grouped by domain
│   │   ├── auth.rs
│   │   ├── accounts.rs
│   │   ├── transactions.rs
│   │   └── labels.rs
│   ├── services/            # Business logic
│   │   ├── enable_banking.rs  # Enable Banking API client
│   │   ├── sync.rs            # Transaction sync orchestration
│   │   ├── labeling.rs        # Auto-labeling engine
│   │   └── notify.rs          # Telegram notifications
│   ├── auth/                # Authentication & session management
│   └── templates/           # Askama HTML templates
├── static/                  # CSS, JS (htmx), icons
├── Cargo.toml
├── .env.example             # Example environment variables
├── CLAUDE.md
└── deploy.sh                # Cross-compile + SCP + restart script
```

## Database Schema

### accounts

| Column          | Type    | Notes                              |
|-----------------|---------|------------------------------------|
| id              | INTEGER | PK, autoincrement                  |
| user_id         | INTEGER | FK → users                         |
| bank_name       | TEXT    | Human-readable name                |
| iban            | TEXT    | Nullable (not all accounts have IBAN) |
| enable_banking_id | TEXT  | Session/requisition ID from Enable Banking |
| access_token_enc | BLOB   | AES-256-GCM encrypted OAuth token  |
| token_expires_at | TEXT   | ISO 8601 datetime                  |
| created_at      | TEXT    | ISO 8601                           |

### transactions

| Column          | Type    | Notes                              |
|-----------------|---------|------------------------------------|
| id              | INTEGER | PK, autoincrement                  |
| account_id      | INTEGER | FK → accounts                      |
| external_id     | TEXT    | Transaction ID from Enable Banking |
| date            | TEXT    | Booking date, ISO 8601             |
| amount          | REAL    | Signed (negative = debit)          |
| currency        | TEXT    | ISO 4217 (EUR, USD, etc.)          |
| description     | TEXT    |                                    |
| counterparty    | TEXT    | Nullable                           |
| balance_after   | REAL    | Nullable                           |
| created_at      | TEXT    | When we first stored it            |
| UNIQUE(external_id, account_id) |  | Deduplication constraint  |

### labels

| Column | Type    | Notes             |
|--------|---------|-------------------|
| id     | INTEGER | PK, autoincrement |
| user_id| INTEGER | FK → users        |
| name   | TEXT    | NOT NULL          |
| color  | TEXT    | Hex color, e.g. #4CAF50 |
| UNIQUE(user_id, name) | | |

### label_rules

| Column    | Type    | Notes                                  |
|-----------|---------|----------------------------------------|
| id        | INTEGER | PK, autoincrement                      |
| label_id  | INTEGER | FK → labels                            |
| field     | TEXT    | 'description', 'counterparty', 'amount_range' |
| pattern   | TEXT    | Regex or keyword for text fields; "min,max" for amount_range |
| priority  | INTEGER | Higher wins on conflict, default 0     |

### transaction_labels

| Column         | Type    | Notes                      |
|----------------|---------|----------------------------|
| transaction_id | INTEGER | FK → transactions          |
| label_id       | INTEGER | FK → labels                |
| source         | TEXT    | 'auto' or 'manual'        |
| PRIMARY KEY (transaction_id, label_id) | | |

### users

| Column        | Type    | Notes                     |
|---------------|---------|---------------------------|
| id            | INTEGER | PK, autoincrement         |
| username      | TEXT    | UNIQUE, NOT NULL          |
| password_hash | TEXT    | Argon2 hash               |
| created_at    | TEXT    | ISO 8601                  |

### sessions

| Column     | Type    | Notes                        |
|------------|---------|------------------------------|
| token      | TEXT    | PK, random 256-bit hex       |
| user_id    | INTEGER | FK → users                   |
| expires_at | TEXT    | ISO 8601                     |
| created_at | TEXT    | ISO 8601                     |

## Authentication Flow

1. User submits username + password to `POST /login`.
2. Server verifies password against Argon2 hash.
3. Server creates a session row and returns a `Set-Cookie: session=<token>; HttpOnly; Secure; SameSite=Strict`.
4. Subsequent requests include the cookie. Axum middleware validates the session.
5. `POST /logout` deletes the session row.

## Bank Linking Flow

1. User clicks "Add bank account" in the UI.
2. `POST /accounts/link` → backend calls Enable Banking to start an authorization session.
3. Backend redirects user to Enable Banking → bank's SCA page.
4. User authenticates with their bank.
5. Bank redirects back to `GET /accounts/callback?code=...`.
6. Backend exchanges the code for an access token, encrypts it, and stores it.
7. Backend fetches initial transactions.

## Sync Job Flow (cron)

```
leanfin sync
  │
  ├─ For each account:
  │   ├─ Decrypt access token
  │   ├─ If expired or expiring within 7 days:
  │   │   ├─ Mark account as "needs_reauth"
  │   │   ├─ Send Telegram notification
  │   │   └─ Skip
  │   ├─ Fetch transactions (last 5 days for overlap)
  │   ├─ INSERT OR IGNORE (dedup by external_id + account_id)
  │   └─ Run auto-labeling rules on newly inserted transactions
  │
  └─ Log summary: "Synced 42 new transactions across 3 accounts"
```

## Deployment

See [deployment.md](deployment.md) for detailed instructions.

Development machine and server are separate. The workflow is:

1. Develop and test locally (using a local SQLite DB).
2. Cross-compile for the target (e.g. `aarch64-unknown-linux-gnu`).
3. Deploy the binary to the server via `deploy.sh`.
4. systemd restarts the service automatically.

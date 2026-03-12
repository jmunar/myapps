# MyApps — Architecture

## Tech Stack

| Layer            | Choice                          |
|------------------|---------------------------------|
| Language         | Rust                            |
| HTTP framework   | Axum                            |
| Database         | SQLite (via sqlx, runtime-checked queries) |
| Frontend         | HTMX + server-rendered HTML     |
| Auth             | Argon2 + server-side sessions   |
| Bank aggregator  | Enable Banking PSD2 API         |
| Notifications    | Telegram Bot API                |
| Reverse proxy    | nginx + certbot                 |
| Process manager  | systemd                         |

## Binary Structure

A single binary with subcommands:

```
myapps serve                # Start the HTTP server
myapps sync                 # Fetch transactions from all linked accounts (cron)
myapps create-user          # Create a user from the command line
myapps seed --app leanfin   # Populate LeanFin demo data
```

All subcommands share the same configuration and database.

## Project Layout

```
myapps/
├── docs/                    # Documentation
├── migrations/              # SQLite migrations (sqlx)
├── src/
│   ├── main.rs              # CLI entrypoint (clap subcommands)
│   ├── config.rs            # Configuration (env vars)
│   ├── db.rs                # Database pool and migrations
│   ├── layout.rs            # Shared HTML layout helper
│   ├── models/              # Domain types (Transaction, Account, Label, etc.)
│   ├── auth/                # Authentication & session management
│   ├── routes/              # Top-level router, auth routes, app launcher
│   │   ├── mod.rs           # Router setup, AppState, nests sub-apps
│   │   ├── auth.rs          # Login/logout
│   │   └── launcher.rs      # App launcher page (root /)
│   ├── services/            # Shared services
│   │   └── notify.rs        # Telegram notifications
│   └── apps/                # Sub-applications
│       └── leanfin/         # LeanFin expense tracker
│           ├── mod.rs       # LeanFin router
│           ├── dashboard.rs # Main transactions page
│           ├── transactions.rs # Transaction list + allocation editor
│           ├── accounts.rs  # Bank account linking (OAuth flow)
│           ├── labels.rs    # Label CRUD
│           └── services/    # LeanFin-specific business logic
│               ├── enable_banking.rs  # Enable Banking API client + JWT
│               ├── sync.rs            # Transaction sync orchestration
│               ├── labeling.rs        # Auto-labeling engine
│               └── seed.rs            # Demo data seeding
├── static/                  # CSS, JS (htmx)
├── Cargo.toml
├── .env.example             # Example environment variables
├── CLAUDE.md
└── deploy.sh                # Rsync + build on server + restart script
```

## Routing Structure

After login, the top-level router serves:

- `/` — App launcher (grid of available apps)
- `/login`, `/logout` — Authentication (public)
- `/leanfin/` — LeanFin sub-app (nested router)
  - `/leanfin/` — Transactions dashboard
  - `/leanfin/transactions` — Transaction list (HTMX partial)
  - `/leanfin/accounts` — Bank account management
  - `/leanfin/labels` — Label CRUD

## Database Schema

### users

| Column        | Type    | Notes                     |
|---------------|---------|---------------------------|
| id            | INTEGER | PK, autoincrement         |
| username      | TEXT    | UNIQUE, NOT NULL          |
| password_hash | TEXT    | Argon2 hash               |
| created_at    | TEXT    | ISO 8601                  |

### sessions (app login sessions)

| Column     | Type    | Notes                        |
|------------|---------|------------------------------|
| token      | TEXT    | PK, random 256-bit hex       |
| user_id    | INTEGER | FK → users                   |
| expires_at | TEXT    | ISO 8601                     |
| created_at | TEXT    | ISO 8601                     |

### accounts

| Column             | Type    | Notes                                     |
|--------------------|---------|-------------------------------------------|
| id                 | INTEGER | PK, autoincrement                         |
| user_id            | INTEGER | FK → users                                |
| bank_name          | TEXT    | Bank name as registered in Enable Banking |
| bank_country       | TEXT    | ISO 3166-1 alpha-2 (e.g. ES, DE)         |
| iban               | TEXT    | Nullable                                  |
| session_id         | TEXT    | Enable Banking session ID (consent)       |
| account_uid        | TEXT    | Enable Banking account UID, UNIQUE        |
| session_expires_at | TEXT    | ISO 8601, when consent expires            |
| created_at         | TEXT    | ISO 8601                                  |

### pending_links

| Column     | Type    | Notes                                |
|------------|---------|--------------------------------------|
| state      | TEXT    | PK, CSRF token for OAuth callback    |
| user_id    | INTEGER | FK → users                           |
| bank_name  | TEXT    | Bank being linked                    |
| country    | TEXT    | Country code                         |
| created_at | TEXT    | ISO 8601                             |

### transactions

| Column          | Type    | Notes                              |
|-----------------|---------|------------------------------------|
| id              | INTEGER | PK, autoincrement                  |
| account_id      | INTEGER | FK → accounts                      |
| external_id     | TEXT    | Transaction ID from Enable Banking |
| date            | TEXT    | Booking date, ISO 8601             |
| amount          | REAL    | Signed (negative = debit)          |
| currency        | TEXT    | ISO 4217 (EUR, USD, etc.)          |
| description     | TEXT    | From remittance information        |
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
| pattern   | TEXT    | Keyword for text fields; "min,max" for amount_range |
| priority  | INTEGER | Higher wins on conflict, default 0     |

### transaction_labels

| Column         | Type    | Notes                      |
|----------------|---------|----------------------------|
| transaction_id | INTEGER | FK → transactions          |
| label_id       | INTEGER | FK → labels                |
| source         | TEXT    | 'auto' or 'manual'        |
| PRIMARY KEY (transaction_id, label_id) | | |

## Authentication Flow

1. User submits username + password to `POST /login`.
2. Server verifies password against Argon2 hash.
3. Server creates a session row and returns a `Set-Cookie: session=<token>; HttpOnly; Secure; SameSite=Lax`.
4. Subsequent requests include the cookie. Axum middleware validates the session.
5. `GET /logout` deletes the session row and clears the cookie.

## Enable Banking Integration

### API Authentication

Enable Banking does **not** use OAuth client credentials. Instead, the app
signs its own JWTs using a private RSA key:

- **Header**: `{"typ":"JWT", "alg":"RS256", "kid":"<app_id>"}`
- **Claims**: `{"iss":"enablebanking.com", "aud":"api.enablebanking.com", "iat":..., "exp":...}`
- **Max TTL**: 24 hours (we use 1 hour)
- A fresh JWT is generated per API call

The private key (`.pem` file) is stored on the server at the path specified
by `ENABLE_BANKING_KEY_PATH`.

### Bank Linking Flow

1. User navigates to `/leanfin/accounts/link` and submits country + bank name.
2. `POST /leanfin/accounts/link` creates a CSRF state token in `pending_links`, then
   calls Enable Banking `POST /auth` to start authorization.
3. User is redirected to Enable Banking → bank's SCA page.
4. User authenticates with their bank (2FA, biometrics, etc.).
5. Bank redirects back to `GET /leanfin/accounts/callback?code=...&state=...`.
6. Backend validates the CSRF state, calls `POST /sessions` to exchange the
   code for a session.
7. The session response includes a list of accounts (each with a `uid`). All
   accounts are stored in the `accounts` table with the `session_id` and
   `session_expires_at`.
8. User is redirected to `/leanfin/accounts`.

### Sync Job Flow (cron)

```
myapps sync
  │
  ├─ Sign a fresh JWT using the private key
  │
  ├─ For each account:
  │   ├─ Check session_expires_at
  │   ├─ If expired:
  │   │   ├─ Send Telegram notification
  │   │   └─ Skip
  │   ├─ If expiring within 7 days:
  │   │   └─ Send Telegram warning
  │   ├─ GET /accounts/{uid}/transactions (last 5 days, paginated)
  │   ├─ INSERT OR IGNORE (dedup by external_id + account_id)
  │   └─ Run auto-labeling rules on newly inserted transactions
  │
  └─ Log summary: "Synced 42 new transactions across 3 accounts"
```

## Deployment

See [deployment.md](deployment.md) for detailed instructions.

Development machine and server are separate. The workflow is:

1. Develop and test locally (using a local SQLite DB).
2. `./deploy.sh deploy` rsyncs source to the Odroid, builds natively, and
   installs + restarts the service.

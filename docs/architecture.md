# MyApps — Architecture

## Tech Stack

| Layer            | Choice                          |
|------------------|---------------------------------|
| Language         | Rust                            |
| HTTP framework   | Axum                            |
| Database         | SQLite (via sqlx, runtime-checked queries) |
| Frontend         | HTMX + server-rendered HTML     |
| Charts           | Frappe Charts 1.6.2 (client-side)|
| Auth             | Argon2 + server-side sessions   |
| Bank aggregator  | Enable Banking PSD2 API         |
| Notifications    | ntfy (HTTP push)                |
| Reverse proxy    | nginx + certbot                 |
| Process manager  | systemd                         |

## Binary Structure

A single binary with subcommands:

```
myapps serve                # Start the HTTP server
myapps sync                 # Fetch transactions from all linked accounts (cron)
myapps create-user          # Create a user from the command line
myapps seed --app leanfin           # Populate LeanFin demo data
myapps seed --app leanfin --reset   # Wipe and re-seed demo data
```

All subcommands share the same configuration and database.

## Project Layout

```
myapps/
├── docs/                    # Documentation
├── migrations/              # SQLite migrations (sqlx)
├── tests/                   # Integration tests (axum-test)
│   ├── harness/mod.rs       # Test harness: in-memory DB, login helpers
│   ├── auth_tests.rs        # Platform auth flow tests
│   ├── leanfin.rs           # LeanFin test binary entry point
│   └── leanfin/             # LeanFin app tests (mirrors src/apps/leanfin/)
│       ├── accounts.rs      # Account list + balance display tests
│       ├── manual_accounts.rs # Manual account CRUD + value update tests
│       ├── transactions.rs  # Dashboard, transaction list/filter tests
│       ├── labels.rs        # Label CRUD + rules tests
│       ├── expenses.rs      # Expenses page + chart endpoint tests
│       └── sync.rs          # Sync button + endpoint tests
├── src/
│   ├── lib.rs               # Library crate (re-exports modules for tests)
│   ├── main.rs              # CLI entrypoint (clap subcommands)
│   ├── config.rs            # Configuration (env vars)
│   ├── db.rs                # Database pool and migrations
│   ├── layout.rs            # Shared HTML layout helper
│   ├── models/              # Domain types (Transaction, Account, Label, etc.)
│   ├── auth/                # Authentication & session management
│   ├── routes/              # Top-level router, auth routes, app launcher
│   │   ├── mod.rs           # Router setup, AppState, build_router(), nests sub-apps
│   │   ├── auth.rs          # Login/logout
│   │   └── launcher.rs      # App launcher page (root /)
│   ├── services/            # Shared services
│   │   └── notify.rs        # ntfy push notifications
│   └── apps/                # Sub-applications
│       └── leanfin/         # LeanFin expense tracker
│           ├── mod.rs       # LeanFin router
│           ├── dashboard.rs # Main transactions page
│           ├── transactions.rs # Transaction list + allocation editor
│           ├── accounts.rs  # Bank account linking (OAuth flow) + manual accounts CRUD
│           ├── labels.rs    # Label CRUD
│           ├── sync_handler.rs  # Sync button endpoint (POST /sync)
│           ├── balance_evolution.rs  # Balance evolution page (Frappe Charts)
│           ├── expenses.rs  # Expenses page: label selector + chart + txn list
│           └── services/    # LeanFin-specific business logic
│               ├── enable_banking.rs  # Enable Banking API client + JWT
│               ├── sync.rs            # Transaction sync orchestration
│               ├── balance.rs         # Daily balance tracking + reconciliation
│               ├── expenses.rs        # Expense aggregation by label + date
│               ├── labeling.rs        # Auto-labeling engine
│               └── seed.rs            # Demo data seeding
├── static/                  # CSS, JS (htmx, frappe-charts)
├── .claude/agents/          # Claude Code agent prompts
│   └── frontend-tester.md   # Agent for generating integration tests
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
  - `/leanfin/accounts` — Account management (bank + manual)
  - `POST /leanfin/accounts/{id}/reauth` — Re-authorize expired bank session
  - `POST /leanfin/accounts/{id}/delete` — Delete account and its data
  - `/leanfin/accounts/manual/new` — Create a manual account (GET form, POST submit)
  - `/leanfin/accounts/manual/{id}/edit` — Edit manual account metadata (GET form, POST submit)
  - `/leanfin/accounts/manual/{id}/value` — Record a new value for a manual account (GET form, POST submit)
  - `POST /leanfin/sync` — Trigger transaction sync for the user (HTMX partial)
  - `/leanfin/balance-evolution` — Balance evolution page (Frappe Charts line chart)
  - `/leanfin/balance-evolution/data?account_id=&days=90` — Balance chart data (HTMX)
  - `/leanfin/expenses` — Expenses page (multi-label selector + chart + transaction list)
  - `/leanfin/expenses/chart?label_ids=1,2&days=90` — Expense chart data (HTMX)
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
| bank_name          | TEXT    | Bank name (or account name for manual)    |
| bank_country       | TEXT    | ISO 3166-1 alpha-2 (empty for manual)     |
| iban               | TEXT    | Nullable                                  |
| session_id         | TEXT    | Enable Banking session ID (placeholder for manual) |
| account_uid        | TEXT    | Enable Banking account UID, UNIQUE (generated UUID for manual) |
| balance_amount     | REAL    | Nullable, latest balance                  |
| balance_currency   | TEXT    | Nullable, ISO 4217 currency               |
| session_expires_at | TEXT    | ISO 8601, when consent expires            |
| account_type       | TEXT    | 'bank' or 'manual', default 'bank'       |
| account_name       | TEXT    | Nullable, display name for manual accounts |
| asset_category     | TEXT    | Nullable, e.g. investment, real_estate, vehicle, loan, crypto |
| created_at         | TEXT    | ISO 8601                                  |

### pending_links

| Column     | Type    | Notes                                |
|------------|---------|--------------------------------------|
| state              | TEXT    | PK, CSRF token for OAuth callback              |
| user_id            | INTEGER | FK → users                                     |
| bank_name          | TEXT    | Bank being linked                              |
| country            | TEXT    | Country code                                   |
| reauth_account_id  | INTEGER | Nullable, FK → accounts (set for re-auth flow) |
| created_at         | TEXT    | ISO 8601                                       |

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

### daily_balances

| Column     | Type    | Notes                                    |
|------------|---------|------------------------------------------|
| id         | INTEGER | PK, autoincrement                        |
| account_id | INTEGER | FK → accounts                            |
| date       | TEXT    | ISO 8601 date                            |
| balance    | REAL    | End-of-day balance                       |
| source     | TEXT    | 'computed', 'reported', 'carried'        |
| created_at | TEXT    | ISO 8601                                 |
| UNIQUE(account_id, date) | | One row per account per day    |

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
  ├─ For each bank account (account_type = 'bank', manual accounts are skipped):
  │   ├─ Check session_expires_at
  │   ├─ If expired:
  │   │   ├─ Send ntfy notification
  │   │   └─ Skip
  │   ├─ If expiring within 7 days:
  │   │   └─ Send ntfy warning
  │   ├─ GET /accounts/{uid}/transactions (last 5 days, paginated)
  │   ├─ INSERT OR IGNORE (dedup by external_id + account_id)
  │   ├─ GET /accounts/{uid}/balances → pick best balance type → UPDATE accounts
  │   ├─ If no daily_balances rows exist → backfill ~90 days from transactions
  │   ├─ Else → reconciliation check (expected vs reported balance, ntfy alert if off)
  │   ├─ Upsert today's daily_balance as 'reported'
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

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
| Speech-to-text   | whisper.cpp (via CLI subprocess) |
| Audio conversion | ffmpeg                          |
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
│       ├── csv_import.rs     # CSV import for manual accounts tests
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
│   │   ├── pwa.rs           # PWA manifest + service worker endpoints
│   │   └── launcher.rs      # App launcher page (root /)
│   ├── services/            # Shared services
│   │   └── notify.rs        # ntfy push notifications
│   └── apps/                # Sub-applications
│       ├── leanfin/         # LeanFin expense tracker
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
│               ├── balance.rs         # Balance snapshots, series computation + reconciliation
│               ├── csv_import.rs      # CSV bulk import for manual account balances
│               ├── expenses.rs        # Expense aggregation by label + date
│               ├── labeling.rs        # Auto-labeling engine
│               └── seed.rs            # Demo data seeding
│       ├── mindflow/        # MindFlow thought capture + mind map
│       │   ├── mod.rs       # MindFlow router + nav
│       │   ├── mind_map.rs  # Mind map page (D3.js) + map data JSON endpoint
│       │   ├── categories.rs # Category CRUD
│       │   ├── thoughts.rs  # Thought capture, detail, comments, actions
│       │   ├── inbox.rs     # Inbox (uncategorized thoughts) + bulk recategorize
│       │   ├── actions.rs   # Actions list, toggle, delete
│       │   └── services/
│       │       └── seed.rs  # Demo data seeding
│       └── voice_to_text/   # VoiceToText audio transcription
│           ├── mod.rs       # VoiceToText router
│           ├── dashboard.rs # Job list page + nav helper
│           ├── jobs.rs      # Upload form, recording, job detail, HTMX partials
│           └── services/
│               ├── transcriber.rs  # ffmpeg conversion + whisper-cli subprocess
│               └── worker.rs       # Background job worker (polls pending jobs)
├── static/                  # CSS, JS (htmx, frappe-charts, d3), PWA assets (icon, sw.js, manifest)
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
- `/manifest.json` — PWA manifest (dynamic, base_path-aware)
- `/sw.js` — Service worker (dynamic, base_path injected)
- `/login`, `/logout` — Authentication (public)
- `/leanfin/` — LeanFin sub-app (nested router)
  - `/leanfin/` — Transactions dashboard
  - `/leanfin/transactions` — Transaction list (HTMX partial)
  - `/leanfin/accounts` — Account management (bank + manual)
  - `POST /leanfin/accounts/{id}/reauth` — Re-authorize expired bank session
  - `POST /leanfin/accounts/{id}/delete` — Delete account and its data
  - `POST /leanfin/accounts/{id}/archive` — Archive account (blocked if unallocated transactions)
  - `POST /leanfin/accounts/{id}/unarchive` — Unarchive account
  - `/leanfin/accounts/manual/new` — Create a manual account (GET form, POST submit)
  - `/leanfin/accounts/manual/{id}/edit` — Edit manual account metadata (GET form, POST submit)
  - `/leanfin/accounts/manual/{id}/value` — Record a new value for a manual account (GET form, POST submit)
  - `/leanfin/accounts/manual/{id}/import-csv` — Bulk-import balance history from CSV (GET form, POST multipart upload)
  - `POST /leanfin/sync` — Trigger transaction sync for the user (HTMX partial)
  - `/leanfin/balance-evolution` — Balance evolution page (Frappe Charts line chart)
  - `/leanfin/balance-evolution/data?account_id=&days=90` — Balance chart data (HTMX)
  - `/leanfin/expenses` — Expenses page (multi-label selector + chart + transaction list)
  - `/leanfin/expenses/chart?label_ids=1,2&days=90` — Expense chart data (HTMX)
  - `/leanfin/labels` — Label CRUD
- `/voice/` — VoiceToText sub-app (nested router)
  - `/voice/` — Job list dashboard (auto-polls for status updates via HTMX)
  - `/voice/new` — Upload form + browser mic recording (MediaRecorder API)
  - `POST /voice/upload` — Multipart file upload, queues transcription job
  - `/voice/jobs/list` — HTMX partial for polling job status updates
  - `/voice/jobs/{id}` — Job detail with transcription text + retry with different model
  - `POST /voice/jobs/{id}/delete` — Delete job and audio file (HTMX partial)
  - `POST /voice/jobs/{id}/retry` — Re-transcribe with a different model (redirects to jobs list)
- `/mindflow/` — MindFlow sub-app (nested router)
  - `/mindflow/` — Mind map page (D3.js visualization + quick capture)
  - `/mindflow/map-data` — Mind map JSON data (categories + thoughts as nodes/links)
  - `/mindflow/categories` — Category CRUD
  - `POST /mindflow/categories/create` — Create category
  - `POST /mindflow/categories/{id}/edit` — Edit category
  - `POST /mindflow/categories/{id}/archive` — Archive category
  - `POST /mindflow/categories/{id}/unarchive` — Unarchive category
  - `POST /mindflow/categories/{id}/delete` — Delete category
  - `POST /mindflow/capture` — Quick thought capture (HTMX partial)
  - `/mindflow/thoughts/{id}` — Thought detail (comments, actions, recategorize)
  - `POST /mindflow/thoughts/{id}/comment` — Add comment (HTMX partial)
  - `POST /mindflow/thoughts/{id}/archive` — Toggle thought archive status
  - `POST /mindflow/thoughts/{id}/recategorize` — Change thought category
  - `POST /mindflow/thoughts/{id}/action` — Create action from thought
  - `POST /mindflow/thoughts/{id}/sub-thought` — Create nested sub-thought
  - `/mindflow/inbox` — Uncategorized thoughts list
  - `POST /mindflow/inbox/recategorize` — Bulk recategorize selected thoughts
  - `/mindflow/actions` — All actions list
  - `POST /mindflow/actions/{id}/toggle` — Toggle action done/pending
  - `POST /mindflow/actions/{id}/delete` — Delete action

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

### leanfin_accounts

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
| archived           | INTEGER | 0 or 1, default 0. Archived accounts are read-only |
| created_at         | TEXT    | ISO 8601                                  |

### leanfin_pending_links

| Column     | Type    | Notes                                |
|------------|---------|--------------------------------------|
| state              | TEXT    | PK, CSRF token for OAuth callback              |
| user_id            | INTEGER | FK → users                                     |
| bank_name          | TEXT    | Bank being linked                              |
| country            | TEXT    | Country code                                   |
| reauth_account_id  | INTEGER | Nullable, FK → accounts (set for re-auth flow) |
| created_at         | TEXT    | ISO 8601                                       |

### leanfin_transactions

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
| snapshot_id     | INTEGER | Nullable FK → balance_snapshots (ON DELETE SET NULL) |
| UNIQUE(external_id, account_id) |  | Deduplication constraint  |

### leanfin_labels

| Column | Type    | Notes             |
|--------|---------|-------------------|
| id     | INTEGER | PK, autoincrement |
| user_id| INTEGER | FK → users        |
| name   | TEXT    | NOT NULL          |
| color  | TEXT    | Hex color, e.g. #4CAF50 |
| UNIQUE(user_id, name) | | |

### leanfin_label_rules

| Column    | Type    | Notes                                  |
|-----------|---------|----------------------------------------|
| id        | INTEGER | PK, autoincrement                      |
| label_id  | INTEGER | FK → labels                            |
| field     | TEXT    | 'description', 'counterparty', 'amount_range' |
| pattern   | TEXT    | Keyword for text fields; "min,max" for amount_range |
| priority  | INTEGER | Higher wins on conflict, default 0     |

### leanfin_balance_snapshots

| Column       | Type    | Notes                                          |
|--------------|---------|------------------------------------------------|
| id           | INTEGER | PK, autoincrement                              |
| account_id   | INTEGER | FK → accounts                                  |
| timestamp    | TEXT    | Full ISO 8601 datetime of the snapshot         |
| date         | TEXT    | Date portion (YYYY-MM-DD), redundant for indexing |
| balance      | REAL    | Balance at this point in time                  |
| balance_type | TEXT    | ITAV, CLAV, XPCD, ITBD, CLBD, or MANUAL       |
| created_at   | TEXT    | ISO 8601                                       |
| UNIQUE(account_id, balance_type, timestamp) | | |

### leanfin_api_payloads

| Column        | Type     | Notes                                          |
|---------------|----------|------------------------------------------------|
| id            | INTEGER  | PK, autoincrement                              |
| account_id    | INTEGER  | Nullable, FK → accounts (ON DELETE SET NULL)   |
| provider      | TEXT     | NOT NULL, default 'enable_banking'             |
| method        | TEXT     | NOT NULL, 'GET' or 'POST'                      |
| endpoint      | TEXT     | NOT NULL, e.g. '/accounts/{uid}/transactions'  |
| request_body  | TEXT     | Nullable, JSON string (NULL for GET requests)  |
| response_body | TEXT     | Nullable, raw JSON response                    |
| status_code   | INTEGER  | NOT NULL, HTTP status code                     |
| duration_ms   | INTEGER  | NOT NULL, round-trip time in milliseconds      |
| created_at    | DATETIME | NOT NULL, default now                          |

Indexes: `account_id`, `created_at`.

### leanfin_transaction_labels

| Column         | Type    | Notes                      |
|----------------|---------|----------------------------|
| transaction_id | INTEGER | FK → transactions          |
| label_id       | INTEGER | FK → labels                |
| source         | TEXT    | 'auto' or 'manual'        |
| PRIMARY KEY (transaction_id, label_id) | | |

### mindflow_categories

| Column     | Type    | Notes                                  |
|------------|---------|----------------------------------------|
| id         | INTEGER | PK, autoincrement                      |
| user_id    | INTEGER | FK → users                             |
| name       | TEXT    | NOT NULL, UNIQUE(user_id, name)        |
| color      | TEXT    | NOT NULL, default '#6B6B6B'            |
| icon       | TEXT    | Nullable                               |
| parent_id  | INTEGER | Nullable FK → mindflow_categories      |
| archived   | INTEGER | 0 or 1, default 0                      |
| position   | INTEGER | Ordering, default 0                    |
| created_at | TEXT    | ISO 8601                               |

### mindflow_thoughts

| Column            | Type    | Notes                                 |
|-------------------|---------|---------------------------------------|
| id                | INTEGER | PK, autoincrement                     |
| user_id           | INTEGER | FK → users                            |
| category_id       | INTEGER | Nullable FK → mindflow_categories     |
| parent_thought_id | INTEGER | Nullable FK → mindflow_thoughts (nesting) |
| content           | TEXT    | NOT NULL                              |
| status            | TEXT    | 'active' or 'archived'                |
| created_at        | TEXT    | ISO 8601                              |
| updated_at        | TEXT    | ISO 8601                              |

### mindflow_comments

| Column     | Type    | Notes                                  |
|------------|---------|----------------------------------------|
| id         | INTEGER | PK, autoincrement                      |
| thought_id | INTEGER | FK → mindflow_thoughts, ON DELETE CASCADE |
| content    | TEXT    | NOT NULL                               |
| created_at | TEXT    | ISO 8601                               |

### mindflow_actions

| Column       | Type    | Notes                                |
|--------------|---------|--------------------------------------|
| id           | INTEGER | PK, autoincrement                    |
| thought_id   | INTEGER | FK → mindflow_thoughts, ON DELETE CASCADE |
| user_id      | INTEGER | FK → users                           |
| title        | TEXT    | NOT NULL                             |
| due_date     | TEXT    | Nullable, ISO 8601 date              |
| priority     | TEXT    | 'low', 'medium', 'high'             |
| status       | TEXT    | 'pending' or 'done'                  |
| created_at   | TEXT    | ISO 8601                             |
| completed_at | TEXT    | Nullable, set when status → done     |

### voice_jobs

| Column            | Type    | Notes                                        |
|-------------------|---------|----------------------------------------------|
| id                | INTEGER | PK, autoincrement                            |
| user_id           | INTEGER | FK → users                                   |
| status            | TEXT    | 'pending', 'processing', 'done', 'failed'   |
| original_filename | TEXT    | NOT NULL, user-uploaded filename              |
| audio_path        | TEXT    | NOT NULL, path to stored file on disk         |
| transcription     | TEXT    | Nullable, populated when status = 'done'     |
| error_message     | TEXT    | Nullable, populated when status = 'failed'   |
| model_used        | TEXT    | 'tiny' or 'base', default 'base'            |
| duration_secs     | REAL    | Nullable, processing wall time               |
| created_at        | TEXT    | ISO 8601                                     |
| completed_at      | TEXT    | Nullable, set when processing finishes        |

Check constraint: `status != 'done' OR transcription IS NOT NULL`.

## Voice Transcription Flow

```
User uploads audio (or records via browser mic)
  │
  ├─ Axum handler saves file to data/voice_uploads/<uuid>.<ext>
  ├─ INSERT voice_jobs row with status = 'pending'
  │
  └─ Background worker (polls every 5s)
      ├─ Claims oldest pending job (atomic UPDATE...RETURNING)
      ├─ ffmpeg converts to 16kHz mono WAV
      ├─ whisper-cli transcribes using configured model
      ├─ UPDATE voice_jobs with transcription text (or error)
      └─ Send ntfy notification (success or failure)
```

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
  ├─ For each active bank account (account_type = 'bank', archived = 0; manual and archived accounts are skipped):
  │   ├─ Check session_expires_at
  │   ├─ If expired:
  │   │   ├─ Send ntfy notification
  │   │   └─ Skip
  │   ├─ If expiring within 7 days:
  │   │   └─ Send ntfy warning
  │   ├─ GET /accounts/{uid}/balances → pick best type → UPDATE accounts
  │   ├─ Record balance snapshot → get snapshot_id
  │   ├─ GET /accounts/{uid}/transactions (last 5 days, paginated)
  │   ├─ Apply credit_debit_indicator: DBIT → negative, CRDT → positive
  │   ├─ INSERT OR IGNORE with snapshot_id (dedup by external_id + account_id)
  │   ├─ Reconciliation (ITAV only): b1 - b0 == SUM(txns where snapshot_id = b1)
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

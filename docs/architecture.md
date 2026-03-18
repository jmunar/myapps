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
| Notifications    | Web Push API (VAPID)            |
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
myapps generate-vapid-keys  # Generate VAPID key pair for push notifications
myapps seed --app leanfin           # Populate LeanFin demo data
myapps seed --app leanfin --reset   # Wipe and re-seed demo data
myapps seed --app classroom         # Populate ClassroomInput demo data
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
│   ├── i18n/               # Internationalization (compile-time struct-based)
│   │   ├── mod.rs           # Lang enum, Translations struct, t() dispatcher
│   │   ├── en.rs            # English translations (const EN)
│   │   └── es.rs            # Spanish translations (const ES)
│   ├── layout.rs            # Shared HTML layout helper
│   ├── models/              # Domain types (Transaction, Account, Label, etc.)
│   ├── auth/                # Authentication & session management
│   ├── routes/              # Top-level router, auth routes, app launcher
│   │   ├── mod.rs           # Router setup, AppState, build_router(), nests sub-apps
│   │   ├── auth.rs          # Login/logout (with language toggle)
│   │   ├── settings.rs      # Language preference handler (POST /settings/language)
│   │   ├── pwa.rs           # PWA manifest + service worker endpoints
│   │   └── launcher.rs      # App launcher page + visibility config + language selector
│   ├── services/            # Shared services
│   │   └── notify.rs        # Web Push notifications (VAPID)
│   └── apps/                # Sub-applications
│       ├── registry.rs      # App metadata registry (AppInfo, all_apps())
│       ├── leanfin/         # LeanFin expense tracker
│           ├── mod.rs       # LeanFin router
│           ├── dashboard.rs # Main transactions page
│           ├── transactions.rs # Transaction list + allocation editor
│           ├── accounts.rs  # Bank account linking (OAuth flow) + manual accounts CRUD
│           ├── labels.rs    # Label CRUD
│           ├── sync_handler.rs  # Sync button endpoint (POST /sync)
│           ├── balance_evolution.rs  # Balance evolution page (Frappe Charts)
│           ├── expenses.rs  # Expenses page: label selector + chart + txn list
│           ├── settings.rs  # Per-user Enable Banking credentials (encrypted storage, settings UI)
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
│       ├── voice_to_text/   # VoiceToText audio transcription
│       │   ├── mod.rs       # VoiceToText router
│       │   ├── dashboard.rs # Job list page + nav helper
│       │   ├── jobs.rs      # Upload form, recording, job detail, HTMX partials
│       │   └── services/
│       │       ├── transcriber.rs  # ffmpeg conversion + whisper-cli subprocess
│       │       └── worker.rs       # Background job worker (polls pending jobs)
│       └── classroom_input/ # ClassroomInput marks & notes recording
│           ├── mod.rs       # ClassroomInput router + nav
│           ├── classrooms.rs # Classroom CRUD (label + pupil list)
│           ├── form_types.rs # Form type CRUD (column definitions)
│           ├── inputs.rs    # Input grid, CSV save, list, detail, delete
│           └── services/
│               └── seed.rs  # Demo data seeding
├── static/                  # CSS, JS (htmx, frappe-charts, d3), PWA assets (icon, sw.js, manifest)
├── .claude/agents/          # Claude Code agent prompts
│   └── frontend-tester.md   # Agent for generating integration tests
├── .github/
│   ├── workflows/
│   │   ├── ci.yml           # PR/push: fmt check + clippy + tests
│   │   └── audit.yml        # Cargo security audit (weekly + on lock changes)
│   └── dependabot.yml       # Weekly Cargo + Actions dependency updates
├── Cargo.toml
├── Makefile                 # Dev shortcuts: fmt, lint, test, check, audit, build, run
├── rustfmt.toml             # Formatting config (edition 2024)
├── .editorconfig            # Editor-agnostic whitespace/encoding
├── .env.example             # Example environment variables
├── CLAUDE.md
└── deploy.sh                # Rsync + build on server + restart script
```

## Routing Structure

After login, the top-level router serves:

- `/` — App launcher (grid of visible apps, configurable per user)
- `/launcher/edit` — Edit mode: toggle app visibility (HTMX partial)
- `/launcher/grid` — Normal mode grid fragment (HTMX partial)
- `POST /launcher/visibility` — Set app visibility preference (HTMX partial)
- `POST /settings/language` — Set user language preference (redirects back)
- `/manifest.json` — PWA manifest (dynamic, base_path-aware)
- `/sw.js` — Service worker (dynamic, base_path injected, push handlers)
- `/push/vapid-key` — VAPID public key (GET, protected)
- `/push/subscribe` — Register push subscription (POST, protected)
- `/push/unsubscribe` — Remove push subscription (POST, protected)
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
  - `/leanfin/settings` — Enable Banking credentials management (GET form, POST multipart)
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
- `/classroom/` — ClassroomInput sub-app (nested router)
  - `/classroom/` — Input list (all saved inputs)
  - `/classroom/new` — New input page (select classroom + form type, fill grid)
  - `POST /classroom/inputs/create` — Save input as CSV
  - `/classroom/inputs/{id}` — View input detail (read-only table)
  - `POST /classroom/inputs/{id}/delete` — Delete input
  - `/classroom/classrooms` — Classroom list + create form
  - `POST /classroom/classrooms/create` — Create classroom
  - `POST /classroom/classrooms/{id}/delete` — Delete classroom and its inputs
  - `/classroom/form-types` — Form type list + create form
  - `POST /classroom/form-types/create` — Create form type
  - `/classroom/form-types/{id}/edit` — Edit form type (GET form, POST submit)
  - `POST /classroom/form-types/{id}/delete` — Delete form type and its inputs

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

### leanfin_user_settings

| Column                | Type    | Notes                                          |
|-----------------------|---------|-------------------------------------------------|
| user_id               | INTEGER | PK, FK → users, ON DELETE CASCADE               |
| enable_banking_app_id | TEXT    | Nullable, Enable Banking application ID         |
| enable_banking_key    | BLOB    | Nullable, AES-256-GCM encrypted RSA private key (nonce prepended) |
| updated_at            | TEXT    | ISO 8601                                        |

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

### user_app_visibility

| Column  | Type    | Notes                                          |
|---------|---------|-------------------------------------------------|
| user_id | INTEGER | FK → users, part of PK                          |
| app_key | TEXT    | 'leanfin', 'mindflow', 'voice_to_text', part of PK |
| visible | INTEGER | 1 = shown, 0 = hidden, default 1               |

Missing rows default to visible — existing users see no change.

### user_settings

| Column   | Type    | Notes                                          |
|----------|---------|------------------------------------------------|
| user_id  | INTEGER | PK, FK → users, ON DELETE CASCADE              |
| language | TEXT    | 'en' or 'es', default 'en'                     |

### push_subscriptions

| Column     | Type    | Notes                          |
|------------|---------|--------------------------------|
| id         | INTEGER | PK, autoincrement              |
| user_id    | INTEGER | FK → users, ON DELETE CASCADE  |
| endpoint   | TEXT    | NOT NULL, UNIQUE               |
| p256dh     | TEXT    | NOT NULL                       |
| auth       | TEXT    | NOT NULL                       |
| created_at | TEXT    | ISO 8601                       |

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

### classroom_classrooms

| Column     | Type    | Notes                          |
|------------|---------|--------------------------------|
| id         | INTEGER | PK, autoincrement              |
| user_id    | INTEGER | FK → users                     |
| label      | TEXT    | NOT NULL (e.g. "1-A")          |
| pupils     | TEXT    | Newline-separated pupil names  |
| created_at | TEXT    | ISO 8601                       |

### classroom_form_types

| Column       | Type    | Notes                                          |
|--------------|---------|-------------------------------------------------|
| id           | INTEGER | PK, autoincrement                              |
| user_id      | INTEGER | FK → users                                     |
| name         | TEXT    | NOT NULL                                       |
| columns_json | TEXT    | JSON array: `[{"name":"…","type":"text\|number\|bool"}]` |
| created_at   | TEXT    | ISO 8601                                       |
| updated_at   | TEXT    | ISO 8601                                       |

### classroom_inputs

| Column       | Type    | Notes                                 |
|--------------|---------|---------------------------------------|
| id           | INTEGER | PK, autoincrement                     |
| user_id      | INTEGER | FK → users                            |
| classroom_id | INTEGER | FK → classroom_classrooms             |
| form_type_id | INTEGER | FK → classroom_form_types             |
| name         | TEXT    | NOT NULL                              |
| csv_data     | TEXT    | Raw CSV (header + one row per pupil)  |
| created_at   | TEXT    | ISO 8601                              |

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
      └─ Send Web Push notification (success or failure)
```

## Authentication Flow

1. User submits username + password to `POST /login`.
2. Server verifies password against Argon2 hash.
3. Server creates a session row and returns a `Set-Cookie: session=<token>; HttpOnly; Secure; SameSite=Lax`.
4. Subsequent requests include the cookie. Axum middleware validates the session.
5. `GET /logout` deletes the session row and clears the cookie.

## Enable Banking Integration

### Per-User Credentials

Enable Banking credentials are stored **per user** in `leanfin_user_settings`.
Each user configures their own Application ID and RSA private key via
`/leanfin/settings`. The private key PEM content is encrypted at rest using
AES-256-GCM with a server-side `ENCRYPTION_KEY` (32-byte hex env var). The
nonce (12 bytes) is prepended to the ciphertext for storage as a BLOB.

The redirect URI is derived from the global `BASE_URL` config (infrastructure-level).

### API Authentication

Enable Banking does **not** use OAuth client credentials. Instead, the app
signs its own JWTs using the user's private RSA key:

- **Header**: `{"typ":"JWT", "alg":"RS256", "kid":"<app_id>"}`
- **Claims**: `{"iss":"enablebanking.com", "aud":"api.enablebanking.com", "iat":..., "exp":...}`
- **Max TTL**: 24 hours (we use 1 hour)
- A fresh JWT is generated per API call

### Bank Linking Flow

1. User configures Enable Banking credentials at `/leanfin/settings`.
2. User navigates to `/leanfin/accounts/link` and submits country + bank name.
   (If credentials are missing, they are redirected to settings.)
3. `POST /leanfin/accounts/link` fetches the user's credentials, creates a
   CSRF state token in `pending_links`, then calls Enable Banking `POST /auth`.
4. User is redirected to Enable Banking → bank's SCA page.
5. User authenticates with their bank (2FA, biometrics, etc.).
6. Bank redirects back to `GET /leanfin/accounts/callback?code=...&state=...`.
7. Backend validates the CSRF state, fetches the user's credentials, calls
   `POST /sessions` to exchange the code for a session.
8. The session response includes a list of accounts (each with a `uid`). All
   accounts are stored in the `accounts` table with the `session_id` and
   `session_expires_at`.
9. User is redirected to `/leanfin/accounts`.

### Sync Job Flow (cron)

```
myapps sync
  │
  ├─ Group all active bank accounts by user_id
  │
  ├─ For each user:
  │   ├─ Fetch Enable Banking credentials from leanfin_user_settings
  │   ├─ If credentials missing → skip user with warning
  │   ├─ Sign a fresh JWT using the user's private key
  │   │
  │   └─ For each active bank account (account_type = 'bank', archived = 0):
  │       ├─ Check session_expires_at
  │       ├─ If expired:
  │       │   ├─ Send push notification
  │       │   └─ Skip
  │       ├─ If expiring within 7 days:
  │       │   └─ Send push warning
  │       ├─ GET /accounts/{uid}/balances → pick best type → UPDATE accounts
  │       ├─ Record balance snapshot → get snapshot_id
  │       ├─ GET /accounts/{uid}/transactions (last 5 days, paginated)
  │       ├─ Apply credit_debit_indicator: DBIT → negative, CRDT → positive
  │       ├─ INSERT OR IGNORE with snapshot_id (dedup by external_id + account_id)
  │       ├─ Reconciliation (ITAV only): b1 - b0 == SUM(txns where snapshot_id = b1)
  │       └─ Run auto-labeling rules on newly inserted transactions
  │
  └─ Log summary: "Synced 42 new transactions across 3 accounts"
```

## Deployment

See [deployment.md](deployment.md) for detailed instructions.

Development machine and server are separate. The workflow is:

1. Develop and test locally (using a local SQLite DB).
2. `./deploy.sh prod deploy` rsyncs source to the Odroid, builds natively, and
   installs + restarts the service.

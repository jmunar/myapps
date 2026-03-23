# MyApps — Architecture

## Tech Stack

| Layer            | Choice                          |
|------------------|---------------------------------|
| Language         | Rust                            |
| HTTP framework   | Axum                            |
| Database         | SQLite (via sqlx, runtime-checked queries, per-app authorizer isolation) |
| Frontend         | HTMX + server-rendered HTML     |
| Charts           | Frappe Charts 1.6.2 (client-side)|
| Auth             | Argon2 + server-side sessions   |
| Bank aggregator  | Enable Banking PSD2 API         |
| Notifications    | Web Push API (VAPID)            |
| Speech-to-text   | whisper.cpp (via CLI subprocess) |
| LLM inference    | llama.cpp server (HTTP API)     |
| Audio conversion | ffmpeg                          |
| Reverse proxy    | nginx + certbot                 |
| Process manager  | systemd                         |

## Binary Structure

A single binary with subcommands:

```
myapps serve                # Start the HTTP server
myapps cron                 # Run scheduled tasks for all deployed apps (e.g. bank sync)
myapps create-user          # Create a user from the command line
myapps invite               # Generate a single-use invite link (48h)
myapps generate-vapid-keys  # Generate VAPID key pair for push notifications
myapps seed --user <name>           # Seed all apps for a user (cleans existing app data)
myapps delete-user --username <name>            # Delete a user and all their data
myapps delete-user-app-data --username <name>   # Delete all app data (keeps user account)
myapps delete-user-app-data --username <name> --app leanfin  # Delete data for one app only
myapps cleanup-users --days 7       # Delete users inactive for >7 days
```

All subcommands share the same configuration and database. Each app receives
a scoped connection pool whose SQLite authorizer restricts table access to
only the app's own prefixed tables (see *Database Isolation* below).

## Project Layout

The project is a Cargo workspace with separate crates for core infrastructure
and each app.

```
myapps/
├── Cargo.toml               # Workspace root + thin binary crate
├── src/
│   ├── main.rs              # CLI entrypoint (~10 lines, delegates to myapps_core::cli)
│   └── lib.rs               # Re-export facade for test compatibility
├── crates/
│   ├── myapps-core/         # Shared infrastructure
│   │   ├── migrations/      # Core SQLite migrations (auth, sessions, settings)
│   │   └── src/
│   │       ├── auth/        # Authentication & session management
│   │       ├── cli.rs       # CLI parsing + command dispatch (clap)
│   │       ├── command/     # Natural-language command bar (LLM-powered)
│   │       ├── config.rs    # Configuration (env vars)
│   │       ├── db.rs        # Database pool and migrations
│   │       ├── i18n/        # Shared translations (auth, launcher, command bar)
│   │       ├── layout.rs    # Shared HTML layout helper
│   │       ├── models/      # Shared domain types (User, Session, Invite, settings)
│   │       ├── registry.rs  # App trait + registry (AppInfo, deployed_app_instances)
│   │       ├── routes/      # Top-level router, auth routes, launcher, PWA, settings
│   │       └── services/    # Shared services (Web Push, Whisper transcription)
│   ├── myapps-leanfin/      # LeanFin expense tracker
│   │   ├── migrations/      # LeanFin database migrations
│   │   ├── static/style.css # LeanFin CSS (embedded via App::css())
│   │   ├── tests/           # LeanFin integration tests
│   │   └── src/             # Handlers, services, models, i18n, ops
│   ├── myapps-mindflow/     # MindFlow thought capture + mind map
│   │   ├── migrations/
│   │   ├── static/style.css
│   │   ├── tests/
│   │   └── src/
│   ├── myapps-voice-to-text/ # VoiceToText audio transcription
│   │   ├── migrations/
│   │   ├── static/style.css
│   │   ├── tests/
│   │   └── src/
│   ├── myapps-classroom-input/ # ClassroomInput marks & notes recording
│   │   ├── migrations/
│   │   ├── static/style.css
│   │   ├── tests/
│   │   └── src/
│   └── myapps-test-harness/ # Shared test utilities (spawn_app, TestApp)
├── tests/                   # Root integration tests
│   ├── harness/mod.rs       # Root test harness (uses all apps)
│   └── auth_tests.rs        # Platform auth, launcher, settings, invite tests
├── models/                  # Whisper GGML model files (gitignored)
├── static/                  # core.css, JS (htmx, frappe-charts, d3), PWA assets
├── .claude/agents/          # Claude Code agent prompts
├── .github/
│   ├── workflows/           # CI, CD, audit
│   └── dependabot.yml
├── Makefile                 # Dev shortcuts (workspace-wide: fmt, lint, test, check)
├── deploy.sh                # Rsync + build on server + restart script
└── scripts/                 # Screenshot automation
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
- `/invite/{token}` — Invite-link registration (GET form, POST submit; public)
- `/login`, `/logout` — Authentication (public)
- `POST /command/transcribe` — Record audio → whisper transcription (protected, requires whisper models)
- `POST /command/interpret` — Parse natural-language input via LLM (protected, requires `LLAMA_SERVER_URL`)
- `POST /command/execute` — Execute a confirmed command action (protected)
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
| username       | TEXT    | UNIQUE, NOT NULL          |
| password_hash  | TEXT    | Argon2 hash               |
| created_at     | TEXT    | ISO 8601                  |
| last_active_at | TEXT    | Nullable, updated hourly by auth middleware |

### sessions (app login sessions)

| Column     | Type    | Notes                        |
|------------|---------|------------------------------|
| token      | TEXT    | PK, random 256-bit hex       |
| user_id    | INTEGER | FK → users                   |
| expires_at | TEXT    | ISO 8601                     |
| created_at | TEXT    | ISO 8601                     |

### invites

| Column     | Type | Notes                                  |
|------------|------|----------------------------------------|
| token      | TEXT | PK, random 256-bit hex                 |
| expires_at | TEXT | ISO 8601, 48h from creation            |
| used_at    | TEXT | Nullable, set when invite is consumed  |
| created_at | TEXT | ISO 8601                               |

App-specific table schemas live alongside their migrations in each crate's
`migrations/` directory (e.g. `crates/myapps-leanfin/migrations/`).

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

## Database Isolation

All apps share a single SQLite file but each app gets its own scoped
connection pool with an `sqlite3_set_authorizer` callback that enforces
table-level access control:

| Operation | Own tables (`<app>_*`) | Core tables (`users`, `sessions`, …) | Other apps' tables |
|-----------|------------------------|--------------------------------------|--------------------|
| Read      | ✅ Allowed              | ✅ Allowed (needed for FK checks)     | ❌ Denied           |
| Write     | ✅ Allowed              | ❌ Denied                             | ❌ Denied           |
| DDL       | ✅ Allowed              | ❌ Denied                             | ❌ Denied           |

The core pool (unrestricted) is used for auth middleware, admin CLI
commands, and migration runner. App handlers, command-bar dispatch, cron
tasks, seed, and background workers all use scoped pools.

`AppState` holds both the core pool (`pool`) and a map of per-app scoped
pools (`app_pools`). Each app's router is mounted with its own `AppState`
clone where `pool` is swapped with the scoped pool, so app handlers
transparently use the restricted connection without code changes.

## Voice Transcription Flow

```
User uploads audio (or records via browser mic)
  │
  ├─ Axum handler saves file to data/voice_uploads/<uuid>.<ext>
  ├─ INSERT voice_to_text_jobs row with status = 'pending'
  │
  └─ Background worker (polls every 5s)
      ├─ Claims oldest pending job (atomic UPDATE...RETURNING)
      ├─ ffmpeg converts to 16kHz mono WAV
      ├─ whisper-cli transcribes using configured model
      ├─ UPDATE voice_to_text_jobs with transcription text (or error)
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
myapps cron
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
2. Merging to `main` triggers the CD pipeline (`.github/workflows/cd.yml`),
   which deploys to staging (with smoke test) then production (with smoke test).
3. Manual deploys are also possible via `./deploy.sh <env> deploy`, which
   rsyncs source to the Odroid, builds natively, and installs + restarts the
   service.

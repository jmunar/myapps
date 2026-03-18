# MyApps — Requirements

## Overview

MyApps is a multi-app personal platform. After login, users see an app launcher
and can navigate into individual applications. All apps share authentication,
database, layout/styling, and configuration.

### LeanFin (first sub-application)

LeanFin is a personal expense management application that automatically fetches
bank transactions, allows the user to label and categorize them, and provides
visibility into spending patterns.

## Functional Requirements

### Platform

- Single login for all applications (shared user accounts and sessions).
- App launcher page after login showing available applications.
- **App visibility configuration** — users can toggle which apps appear on
  their launcher via an inline edit mode (gear icon). Hidden apps are stored
  per-user; defaults to all visible. Edit mode shows all apps with hidden
  ones dimmed/dashed.
- Shared navigation with brand ("MyApps"), app-level nav, and logout.

### Bank account integration (LeanFin)

- Connect multiple bank accounts via **Enable Banking** (PSD2 aggregator).
- Each bank connection requires manual user authorization (SCA) through the
  bank's login page. Consent validity depends on the bank (typically 90–180
  days).
- A daily cron job fetches new transactions from all linked accounts. Users can
  also trigger a sync manually from the web UI.
- When a bank consent expires (or is close to expiry), the system notifies the
  user so they can re-authorize.

### Transaction management (LeanFin)

- Transactions are stored locally and deduplicated by their external ID
  (provided by Enable Banking) scoped to the account. Amounts are signed using
  the `credit_debit_indicator` from the API (DBIT = negative, CRDT = positive).
- Each transaction records at minimum: date, amount, currency, description,
  counterparty name, and balance after transaction.
- The system never modifies or deletes upstream bank data; it is append-only.
- The transaction list can be filtered by free text (searches description and
  counterparty), by bank account, by label, by date range, and by allocation
  status (showing only transactions that are not fully allocated). All filters
  are reactive and update results as the user types or changes a selection.

### Labeling and allocations (LeanFin)

- Users can create labels (e.g. "Groceries", "Rent", "Salary") with a color.
- Transactions are categorized via **allocations**: each allocation assigns a
  portion of the transaction amount to a label.
- A transaction can have one allocation (simple labeling) or multiple
  (splitting, e.g. a grocery receipt that includes dining items).
- The sum of allocation amounts should equal the transaction's absolute amount.
- Allocations can be created/removed through an inline editor in the
  transaction list.
- Users can define auto-labeling rules (pattern matching on description or
  counterparty). Rules run automatically on newly fetched transactions and
  create a single allocation for the full amount.
- Manual allocations take precedence over auto-assigned ones.

### Notifications

- The system notifies the user when a bank consent is expired or about to
  expire (e.g. 7 days before).
- Notification channel: Web Push (standard browser push notifications via the
  Push API with VAPID authentication). Works on Android, iOS 16.4+, and desktop
  browsers. Users grant permission from the app launcher page.

### User interface

- Web application (responsive, mobile-friendly).
- Progressive Web App (PWA) — installable on mobile and desktop via web app
  manifest, service worker for offline static asset caching and network-first
  HTML page loading.
- Future: Android app (wrap the web app or build native).

## Non-Functional Requirements

### Resource efficiency

- The server is a Raspberry Pi (or equivalent) with very limited RAM and CPU.
- The backend must idle at under 10 MB RSS.
- SQLite is used as the database to avoid a separate DB process.

### Security

- The server is exposed to the internet behind nginx + certbot (HTTPS).
- User authentication via username/password with Argon2 hashing and
  session cookies.
- Enable Banking API uses self-signed JWTs (RS256) for authentication. Private
  keys are stored per-user in the database, encrypted at rest with AES-256-GCM
  using a server-side encryption key.
- No secrets are committed to the repository.

### CI/CD

- GitHub Actions enforces formatting (rustfmt), linting (clippy with
  warnings-as-errors), and tests on every push and pull request.
- Merging to `main` triggers automatic deployment: staging first (with smoke
  test), then production (with smoke test). Deploys use a dedicated SSH user
  with scoped sudo.
- A scheduled security audit (`cargo audit`) runs weekly and on Cargo.toml/lock
  changes.
- Dependabot opens weekly PRs for Cargo dependency updates and GitHub Actions
  version bumps.
- A Makefile provides local shortcuts that mirror CI checks (`make check`) and
  GitHub environment setup (`make gh-env`).

### Deployment

- Development happens on a separate machine (not the server).
- Source is rsynced to the server and compiled natively on the Odroid N2.
- The application runs as a systemd service on the server.
- The cron job is a system crontab entry that invokes the same binary with a
  `sync` subcommand.

### Reliability

- The daily sync job is idempotent: re-fetching overlapping date ranges
  produces no duplicates.
- If the sync job fails (network error, token expired), it logs the error and
  continues with the next account.
- SQLite WAL mode is enabled for safe concurrent reads during sync.

## Roadmap

### Implemented

- **Label rule management UI** — create/edit/delete auto-labeling rules from
  the web interface.
- **Transaction filtering** — filter the transaction list by free text search,
  account, or allocation status (not fully allocated). Filters are reactive
  and trigger on every change via HTMX.
- **Account management** — re-authorize expired bank sessions and delete
  accounts from the UI.
- **Integration test infrastructure** — HTTP-level tests using axum-test with
  in-memory SQLite, covering auth flows, transaction list/filtering, and label
  CRUD. A Claude Code agent automates test generation for new features.
- **Account balances** — fetch and display account balances from Enable Banking
  during sync. Balances are shown on the accounts page and as a running balance
  column in the transaction list.
- **Manual sync button** — a sync button on the Transactions and Accounts pages
  that triggers an on-demand sync of the user's linked bank accounts. Shows
  real-time status feedback: spinning icon during sync, success/error pill badge
  on completion. The transaction list auto-refreshes after sync.
- **Balance evolution tracking** — a dedicated Balance page shows an interactive
  Frappe Charts line chart with period selectors (30d/90d/180d/365d) and an
  account dropdown including an "All accounts" aggregated view. Each sync
  fetches balances first, records a snapshot, then fetches transactions and
  links them to that snapshot via `snapshot_id`. Bank account balances between
  snapshots are interpolated using only the transactions linked to the next
  snapshot, ensuring exact attribution. Clicking a data point on the chart
  shows the transactions for that date below. Manual accounts use stored
  reported values with gap-filling. Balance snapshots store the bank's balance
  type (ITAV, CLAV, XPCD, ITBD, CLBD) with appropriate timestamps: intraday
  types use the sync time, closing types use end-of-day. Reconciliation checks
  use snapshot-linked transactions (`b1 - b0 == SUM(txns where snapshot_id =
  b1)`), running only for ITAV snapshots and alerting via push notification if discrepancies
  exceed 0.01.
- **Manual accounts** — users can create manually tracked accounts for assets not
  accessible through Open Banking (investments, real estate, vehicles, loans,
  crypto). Manual accounts support CRUD operations (create, edit metadata, update
  value with date, delete). Values are recorded as daily balance entries with
  carry-forward gap filling for sparse updates. The accounts page is split into
  "Bank Accounts" and "Manual Accounts" sections. Balance evolution charts and
  the "All accounts" aggregated view include manual accounts seamlessly. The sync
  process filters to bank accounts only, skipping manual accounts. Users can
  bulk-import historical balance data from CSV files (two-column format:
  date + value) with all-or-nothing validation and idempotent upserts.
- **Expense visualization** — a dedicated Expenses page with multi-label
  selection (toggle pills), a Frappe Charts time series showing daily totals per
  selected label (both credit and debit transactions), and a transaction list
  below the chart. When multiple labels are selected, a black "Total" line shows
  the aggregate. Clicking a data point on the chart filters the transaction list
  to that date. The transaction list reuses the same endpoint as the Transactions
  page with label_ids and date range filters.

- **Account archiving** — accounts (bank or manual) can be archived to make them
  read-only. Archived bank accounts are skipped during sync; archived manual
  accounts cannot be edited or have their value updated. Archiving is blocked
  when the account has unallocated transactions. Archived accounts are hidden
  from the accounts list by default (a "Show archived" toggle reveals them) and
  excluded from the balance evolution individual account dropdown, but their
  balances are still included in the aggregated "All accounts" view. Transactions
  from archived accounts remain visible in the transaction list.
- **Pagination** — the transaction list paginates with 50 transactions per page.
  Prev/Next controls appear below the table with a "from–to of total" counter.
  Filters reset to page 1 automatically. Pagination buttons use HTMX with
  `hx-vals` to pass the page number while preserving all active filters via
  `hx-include`.

- **API payload logging** — all Enable Banking API calls (auth, session creation,
  transaction fetches, balance fetches) persist the full request/response bodies,
  HTTP status codes, and round-trip durations to an `api_payloads` table. This
  provides an audit trail for debugging sync issues, reconciliation mismatches,
  and provider behavior changes. Payloads are linked to the relevant account when
  available; pre-account calls (auth, sessions) store a NULL account_id.
- **Per-user Enable Banking credentials** — each user configures their own
  Enable Banking Application ID and RSA private key via a settings page
  (`/leanfin/settings`). Private keys are uploaded as PEM files, validated
  server-side, and stored encrypted (AES-256-GCM) in the database. The
  encryption key is a server-side env var (`ENCRYPTION_KEY`). The "Link bank
  account" button on the accounts page conditionally shows "Configure Enable
  Banking" if the user has not yet set up credentials. The sync job groups
  accounts by user and skips users without configured credentials.

### MindFlow (second sub-application)

MindFlow is a personal thought capture and mind map application. Users jot
thoughts throughout the day, categorize them into topics, and visualize the
connections on an interactive mind map.

#### Implemented

- **Categories CRUD** — predefined categories with name, color, and optional icon.
  Categories can be created, edited, archived, and deleted (if no thoughts
  reference them).
- **Thought capture** — quick inline capture from the mind map page with optional
  category picker. Uncategorized thoughts land in the Inbox.
- **Mind map visualization** — D3.js force-directed graph showing categories as
  large colored nodes and thoughts as smaller linked nodes. Supports zoom, drag,
  and click-to-navigate to thought details.
- **Thought detail** — full view of a thought with category badge, recategorize
  dropdown, archive toggle, inline comment thread (HTMX), action creation, and
  nested sub-thoughts displayed as a recursive tree.
- **Inbox** — list of uncategorized thoughts with bulk recategorize via checkboxes
  and category dropdown.
- **Actions** — to-do items linked to thoughts, with priority (low/medium/high),
  optional due date, toggle done/pending, and delete. Actions page shows all
  actions sorted by status and priority.
- **Seed data** — `cargo run -- seed --app mindflow` populates demo categories,
  thoughts, comments, and actions.

#### Not yet implemented
- **Sub-categories** — hierarchical nesting within categories.
- **Alerts** — overdue actions, stale thoughts.
- **Date range filtering** on the mind map.
- **Web Push integration** for push notifications.
- **Local LLM integration** — async auto-categorization, action extraction,
  connection finding via Llama-3.2-1B-Instruct.

### ClassroomInput (fourth sub-application)

ClassroomInput is a mobile-oriented app for recording marks and notes for
classrooms. Teachers select a classroom and form type, then fill in a
spreadsheet-like grid that saves data as CSV.

#### Implemented

- **Classroom management** — create and delete classrooms. A classroom has a
  label (e.g. "1-A") and a list of pupils pasted from the clipboard (one per
  line, empty lines stripped).
- **Form type management** — create, edit, and delete form types. Each form type
  defines a set of columns with name and type (text, number, or yes/no boolean).
  Columns can be added/removed dynamically in the editor.
- **Input grid** — select a classroom + form type, name the input, then fill a
  table with pupils as the frozen left column and one column per form type field.
  Arrow keys navigate between cells (with wrap-around at row boundaries), Enter
  moves down. On submit, the grid is serialized as CSV and stored in the database.
- **Input list and detail** — all inputs are listed with classroom, form type,
  row count, and date. Each input can be viewed as a read-only HTML table or
  deleted.
- **Seed data** — `cargo run -- seed --app classroom` populates 3 classrooms,
  4 form types, and 4 sample inputs with realistic data.

#### Not yet implemented
- **Input editing** — re-open a saved input for modification.
- **CSV export** — download an input as a CSV file.
- **Bulk operations** — delete multiple inputs at once.

### VoiceToText (third sub-application)

VoiceToText is an audio transcription application that converts speech to text
using whisper.cpp running locally on the Odroid N2. Transcription is async —
users upload audio and are notified when the result is ready.

#### Implemented

- **Audio upload** — multipart file upload accepting common audio formats
  (wav, mp3, ogg, webm, m4a, flac). Files stored in `data/voice_uploads/`.
- **Browser recording** — in-browser microphone capture via the MediaRecorder
  API. Records as webm and uploads directly.
- **Dynamic model selection** — available models are discovered at runtime by
  scanning `WHISPER_MODELS_DIR` for `ggml-*.bin` files. Admin controls which
  models appear by downloading/removing model files. Supports quantized variants
  (e.g. base-q5_1, tiny-q5_1).
- **Background transcription worker** — a tokio task polls for pending jobs
  every 5 seconds, processes one at a time. Converts audio to 16kHz mono WAV
  via ffmpeg, then runs whisper-cli as a subprocess.
- **Job tracking** — each transcription is a job with status
  (pending/processing/done/failed), timing, and error messages. The job list
  auto-polls via HTMX when active jobs exist.
- **Web Push notifications** — browser push notification sent on job completion
  or failure.
- **Job detail page** — view transcription text, processing time, and metadata.
  Includes a re-transcribe form to submit the same audio with a different model
  for comparison.
- **Job deletion** — delete any job (any status) from the jobs list, removing
  the audio file from disk.

#### Not yet implemented
- **Language selection** — currently auto-detected; allow explicit language choice.
- **Max duration enforcement** — limit upload size/duration to bound processing time.
- **Seed data** — demo jobs for development.
- **Integration tests** — frontend tests for upload and job list pages.

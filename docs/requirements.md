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
- **Deployable app subset** — the `DEPLOY_APPS` environment variable
  (comma-separated app keys) limits which apps are mounted, shown in the
  launcher, and have background workers started. When unset, all apps are
  available.
- Shared navigation with brand ("MyApps"), app-level nav, and logout.
- **Voice command bar** — when both a llama.cpp server (`LLAMA_SERVER_URL`) and
  whisper models are available, a floating mic button appears on every page.
  Users press and hold to record a voice command, release to transcribe and
  interpret. The audio is transcribed via whisper-cli, then the LLM parses the
  text into a structured action shown in a floating window for confirmation.
  Users can edit the transcription before the LLM processes it. Swiping left
  while recording discards the input. Per-app `ops.rs` modules provide shared
  action functions used by both the command bar and web handlers.
- **External app shortcuts** — the `EXTERNAL_APPS` environment variable adds
  shortcut tiles to the launcher for services running outside MyApps (e.g.
  Vaultwarden, Cockpit). Each entry specifies a key, name, description, icon,
  and URL. External tiles open in a new browser tab with a visual ↗ badge.
  Users can hide/show external app tiles through the existing edit mode.
- **Version footer** — the launcher page displays the build version and
  timestamp at the bottom, embedded at compile time.
- **Multilingual support (i18n)** — English and Spanish. Users select their
  language on the login page (toggle link) or from a dropdown on the launcher.
  Preference is stored per-user in the database and propagated through the auth
  middleware. Compile-time struct-based translations with zero runtime cost;
  adding a new string field forces both language files to be updated.

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
- Rules can also be created directly from the allocation editor in the
  transaction list. The pattern is pre-filled from the transaction's
  counterparty or description. On creation, the rule is immediately applied
  to all existing unallocated transactions.
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
- **Invite-link user onboarding** — admins generate a single-use,
  time-limited (48h) invite link via the CLI (`myapps invite`). New users
  open the link, choose their own username and password, and are
  automatically logged in. The admin never sees the user's password.
- **Auto-seed on registration** — when `SEED=true`, new users who register
  via invite automatically get demo data seeded for all deployed apps.
- **User deletion** — `delete-user --username <name>` deletes a user and all
  their data (CASCADE). `delete-user-app-data --username <name>` removes only
  app data while keeping the user account; an optional `--app <key>` flag
  targets a single app.
- **Inactive user cleanup** — `cleanup-users --days N` deletes users whose
  last activity was more than N days ago. Activity is tracked via
  `last_active_at` (updated hourly by auth middleware). Used in staging to
  keep the environment clean.
- **Per-app database isolation** — each app's connection pool has a SQLite
  authorizer that restricts writes to its own prefixed tables and denies reads
  on other apps' tables. Core tables are readable (for FK constraint checks)
  but not writable from app pools. This prevents bugs or malicious queries in
  one app from affecting another's data.
- Enable Banking API uses self-signed JWTs (RS256) for authentication. Private
  keys are stored per-user in the database, encrypted at rest with AES-256-GCM
  using a server-side encryption key.
- No secrets are committed to the repository.

### CI/CD

- GitHub Actions enforces formatting (rustfmt), linting (clippy with
  warnings-as-errors), and tests on every push and pull request.
- Merging to `main` triggers automatic deployment: the CD pipeline auto-bumps
  the version, cross-compiles for aarch64, creates a GitHub Release with the
  binary, then deploys to staging and production (with smoke tests). Deploys
  use a dedicated SSH user with scoped sudo.
- A scheduled security audit (`cargo audit`) runs weekly and on Cargo.toml/lock
  changes.
- Dependabot opens weekly PRs for Cargo dependency updates and GitHub Actions
  version bumps.
- A Makefile provides local shortcuts that mirror CI checks (`make check`) and
  GitHub environment setup (`make gh-env`).

### Deployment

- Development happens on a separate machine (not the server).
- Release binaries are cross-compiled in GitHub Actions for aarch64 and
  deployed directly to the server. Manual deploys can still build natively
  on the Odroid via `deploy.sh deploy`.
- The application runs as a systemd service on the server.
- The cron job is a system crontab entry that invokes the same binary with the
  `cron` subcommand, which runs each deployed app's scheduled tasks.

### Reliability

- The daily sync job is idempotent: re-fetching overlapping date ranges
  produces no duplicates.
- If the sync job fails (network error, token expired), it logs the error and
  continues with the next account.
- SQLite WAL mode is enabled for safe concurrent reads during sync.

## Roadmap

### Implemented

- **Label rule management UI** — create/edit/delete auto-labeling rules from
  the web interface, including inline rule creation from the transaction
  allocation editor with immediate application to unallocated transactions.
- **Transaction filtering** — filter the transaction list by free text search,
  account, or allocation status (not fully allocated). Filters are reactive
  and trigger on every change via HTMX.
- **Account management** — re-authorize expired bank sessions and delete
  accounts from the UI.
- **Integration test infrastructure** — HTTP-level tests using axum-test with
  in-memory SQLite, covering all apps: auth flows, launcher, settings, invite
  registration, and per-app route/CRUD testing for LeanFin, MindFlow,
  VoiceToText, and FormInput. A Claude Code agent automates test
  generation for new features. The `/finish-development` command includes a
  frontend test generation step.
- **Account balances** — fetch and display account balances from Enable Banking
  during sync. Balances are shown on the accounts page and as a running balance
  column in the transaction list.
- **Manual sync button** — a sync button on the Transactions and Accounts pages
  that triggers an on-demand sync of the user's linked bank accounts. Shows
  real-time status feedback: spinning icon during sync, success/error pill badge
  on completion. The transaction list auto-refreshes after sync.
- **Balance evolution tracking** — a dedicated Balance page shows an interactive
  Chart.js line chart with period selectors (30d/90d/180d/365d) and an
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
  selection (toggle pills), a Chart.js stacked bar chart showing totals per
  selected label with period-appropriate intervals (daily for 30d, weekly for
  90d, monthly for 180d/365d). Intervals use canonical end dates (Sunday for
  weekly, last day of month for monthly) so bars align across labels. Positive
  expenses stack upward, negative (income/refunds) stack downward from zero.
  Clicking a bar filters the transaction list to the full time window
  represented by that bar. The transaction list reuses the same endpoint as
  the Transactions page with label_ids and date range filters.

- **Account coloring** — each account (bank or manual) can be assigned a custom
  color via an inline color picker on the accounts page. The color appears as a
  left-side stripe on account cards and as a left border on transaction rows,
  providing quick visual identification of which account a transaction belongs
  to. Allocation status (unallocated/misallocated) is shown via background
  color on the transaction row rather than the left border.
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
- **Transaction details viewer** — a "More details" button in the allocation
  editor loads the raw Enable Banking API payload for a transaction. The payload
  is located by matching the transaction's external ID against stored API
  responses, with null fields stripped. The JSON is rendered using a reusable
  collapsible tree component (`myapps_core::components`) that uses
  `<details>`/`<summary>` elements for expand/collapse without JavaScript.
  Seed data includes matching API payloads for demo users.
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
- **Seed data** — `cargo run -- seed --user <name>` populates demo categories,
  thoughts, comments, and actions for a given user.

#### Not yet implemented
- **Sub-categories** — hierarchical nesting within categories.
- **Alerts** — overdue actions, stale thoughts.
- **Date range filtering** on the mind map.
- **Web Push integration** for push notifications.
- **Local LLM integration** — MindFlow-specific: async auto-categorization,
  action extraction, connection finding.

### FormInput (fourth sub-application)

FormInput is a mobile-oriented app for capturing structured data with custom
forms. The user picks a form type (and optionally a row set, depending on the
form type's mode), then fills in a spreadsheet-like grid that saves data as CSV.

#### Implemented

- **Row set management** — create and delete row sets. A row set has a label
  (e.g. "1-A") and a list of row identifiers pasted from the clipboard (one
  per line, empty lines stripped).
- **Form type management** — create, edit, and delete form types. Each form type
  defines a set of columns with name and type (text, number, yes/no boolean, or
  link). A form type also has a `fixed_rows` flag: when on, every input picks
  a row set and gets one row per item; when off, the user adds rows freely.
- **Link column type** — link cells open a small modal for URL + optional
  display text (default "link"/"enlace"). The view renders an anchor; storage
  is a single CSV cell formatted as `url|text`.
- **Input grid** — select a form type (and a row set when the form type is
  fixed-row), name the input, then fill a table. In fixed-row mode the row
  identifiers form a frozen left column; in dynamic mode the user adds and
  removes rows freely. On submit, the grid is serialized as CSV and stored in
  the database.
- **Input editing in place** — the view page reuses the input grid layout.
  Double-click any data cell to edit it; one cell save is persisted via AJAX
  (`POST /forms/inputs/{id}/cell`). The row identifier in fixed-row mode stays
  read-only.
- **Per-column sort and filter on the view** — every column header carries
  small sort buttons (A→Z, Z→A) and a filter input. Sort is exclusive across
  columns, filters stack. Numeric columns sort by parsed value with empty
  cells last. Sorting and filtering are presentation-only — saves still hit
  the underlying CSV row regardless of the visible order.
- **Input list and detail** — all inputs are listed with row set, form type,
  row count, and date. Each input can be viewed (and edited cell by cell) or
  deleted.
- **Seed data** — `cargo run -- seed --user <name>` populates row sets, form
  types (mix of fixed and dynamic), and sample inputs with realistic data
  for a given user.

#### Not yet implemented
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

- **Integration tests** — frontend tests for dashboard, job detail, new form,
  delete, and list partial endpoints using axum-test.

#### Not yet implemented
- **Language selection** — currently auto-detected; allow explicit language choice.
- **Max duration enforcement** — limit upload size/duration to bound processing time.
- **Seed data** — demo jobs for development.

### Notes (fifth sub-application)

Notes is a markdown-based note-taking application with a live WYSIWYG editor
and voice dictation support.

#### Implemented

- **WYSIWYG Markdown editor** — Tiptap editor (StarterKit + Link + TaskList +
  TaskItem + Collaboration + tiptap-markdown) mounted into `#notes-editor`.
  Typed/pasted markdown is parsed on input and round-trips back to markdown
  on read, with a small set of cosmetic transformations (blank lines around
  block elements, normalized bullet markers, task items serialized as `[x]` /
  `[ ]` without the leading `-`). The legacy `- [x]` / `- [ ]` form is still
  accepted on parse for backwards compatibility.
- **Task checkboxes** — Markdown task items render as real `<input
  type="checkbox">` elements. Checking/unchecking persists immediately.
- **Local-first sync (Yjs CRDT)** — body content lives in a Yjs document. Per
  note, a `Y.Doc` is persisted client-side to IndexedDB and shipped between
  peers through a per-note WebSocket relay (`/notes/{uuid}/ws`). Offline
  edits queue locally and converge with the server on reconnect; multiple
  devices editing the same note simultaneously merge without conflicts.
- **Update-log compaction** — the server's append-only
  `notes_note_updates` table is snapshotted into a single row when a room has
  zero subscribers and has been idle for ≥60s, so storage doesn't grow
  monotonically with edits.
- **Offline editor shell (PWA)** — service worker caches the editor HTML, the
  vendored Yjs + Tiptap bundle (~600 KB), and the bootstrap so previously
  visited notes open offline; the cache keys versioned-URLs and rotates on
  every `STATIC_VERSION` bump.
- **Note CRUD** — create, edit, save (title), and delete notes. Title is
  saved via the form POST at `/notes/{id}/save`; body flows through the
  WebSocket. Untitled notes show a placeholder.
- **Pin notes** — pin important notes so they appear at the top of the list.
- **Voice dictation** — when whisper.cpp is configured, a dictate button
  records audio, transcribes it server-side, and `editor.commands
  .insertContent` inserts the text at the cursor; the CRDT propagates the
  edit to other peers.
- **Note list** — grid of note cards showing title, preview text, date, and
  pinned badge. Pinned notes sort first, then by last updated.
- **Seed data** — `cargo run -- seed --user <name>` populates 4 demo notes
  with rich Markdown content (headings, code blocks, lists, blockquotes,
  links).
- **Integration tests** — 41 tests covering auth, CRUD, pin toggle, empty
  state, seeded rendering, the WebSocket sync handshake, two-client
  convergence, and idle-eviction compaction.

#### Not yet implemented
- **Note sharing** — share notes with other users of the app.
- **Full-text search** — search notes by content.
- **Folders/tags** — organize notes into categories.
- **Keep the list-view preview fresh after CRDT edits** — once a note is
  edited via the Tiptap editor, `notes_notes.body` (the column the list view
  reads for its preview line) no longer reflects the current content.
  Options: (a) the bootstrap POSTs the current markdown to a
  `/notes/{id}/denormalize-body` endpoint on `visibilitychange` /
  `beforeunload`; (b) port `tiptap-markdown`'s ProseMirror→markdown
  serializer to Rust and run it during idle eviction. (a) is cheaper; (b) is
  always-on.
- **Migrate pre-CRDT note bodies into the CRDT** — notes created before this
  feature shipped open as empty in the Tiptap editor (the markdown stays in
  `notes_notes.body` for the list preview but isn't replayed into the
  `Y.XmlFragment`). Fix: the bootstrap calls
  `editor.commands.setContent(legacyBody)` on first open when the CRDT is
  empty, gated by a `body_seeded INTEGER` flag on `notes_notes` to avoid
  double-seeding from concurrent first-opens across devices.

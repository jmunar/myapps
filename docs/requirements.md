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
- Notification channel: ntfy (open-source, self-hostable, simple HTTP push).

### User interface

- Web application (responsive, mobile-friendly).
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
- Enable Banking API uses self-signed JWTs (RS256) for authentication. The
  private key (`.pem`) is stored on the server, not in the repository.
- No secrets are committed to the repository.

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
  account dropdown including an "All accounts" aggregated view. Bank account
  balances are computed on the fly from the most recent reported balance and
  transaction sums (no persisted computed rows). Manual accounts use stored
  reported values with gap-filling. Reconciliation checks compare expected vs
  reported balances on each sync, alerting via ntfy if discrepancies exceed 0.01.
- **Manual accounts** — users can create manually tracked accounts for assets not
  accessible through Open Banking (investments, real estate, vehicles, loans,
  crypto). Manual accounts support CRUD operations (create, edit metadata, update
  value with date, delete). Values are recorded as daily balance entries with
  carry-forward gap filling for sparse updates. The accounts page is split into
  "Bank Accounts" and "Manual Accounts" sections. Balance evolution charts and
  the "All accounts" aggregated view include manual accounts seamlessly. The sync
  process filters to bank accounts only, skipping manual accounts.
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

### Not yet implemented
- **Additional apps** — new sub-applications on the platform.

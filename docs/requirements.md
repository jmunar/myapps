# LeanFin — Requirements

## Overview

LeanFin is a personal expense management application that automatically fetches
bank transactions, allows the user to label and categorize them, and provides
visibility into spending patterns.

## Functional Requirements

### Bank account integration

- Connect multiple bank accounts via **Enable Banking** (PSD2 aggregator).
- Each bank connection requires manual user authorization (SCA) through the
  bank's login page. Consent is valid for up to 90 days per PSD2 regulation.
- A daily cron job fetches new transactions from all linked accounts.
- When a bank consent expires (or is close to expiry), the system notifies the
  user so they can re-authorize.

### Transaction management

- Transactions are stored locally and deduplicated by their external ID
  (provided by Enable Banking) scoped to the account.
- Each transaction records at minimum: date, amount, currency, description,
  counterparty name, and balance after transaction.
- The system never modifies or deletes upstream bank data; it is append-only.

### Labeling

- Users can create labels (e.g. "Groceries", "Rent", "Salary") with an
  optional color.
- A transaction can have multiple labels.
- Labels can be applied manually through the UI.
- Users can define auto-labeling rules (pattern matching on description,
  counterparty, or amount range). Rules run automatically on newly fetched
  transactions.
- Manual labels take precedence over auto-assigned ones.

### Notifications

- The system notifies the user when a bank consent is expired or about to
  expire (e.g. 7 days before).
- Notification channel: Telegram bot (lightweight, no infrastructure needed).

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
- Bank OAuth tokens are encrypted at rest (AES-256-GCM) using a key loaded
  from an environment variable.
- No secrets are committed to the repository.

### Deployment

- Development happens on a separate machine (not the Raspberry Pi).
- The binary is cross-compiled for the target architecture (e.g. aarch64) and
  deployed via SSH/SCP.
- The application runs as a systemd service on the server.
- The cron job is a system crontab entry that invokes the same binary with a
  `sync` subcommand.

### Reliability

- The daily sync job is idempotent: re-fetching overlapping date ranges
  produces no duplicates.
- If the sync job fails (network error, token expired), it logs the error and
  continues with the next account.
- SQLite WAL mode is enabled for safe concurrent reads during sync.

-- Rework accounts table for Enable Banking session-based auth.
-- Enable Banking uses session_id + account_uid instead of OAuth tokens.
-- Drop the old table and recreate (no production data yet).

DROP TABLE IF EXISTS transactions;
DROP TABLE IF EXISTS accounts;

CREATE TABLE accounts (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_name          TEXT    NOT NULL,
    bank_country       TEXT    NOT NULL,
    iban               TEXT,
    session_id         TEXT    NOT NULL,
    account_uid        TEXT    NOT NULL UNIQUE,
    session_expires_at TEXT    NOT NULL,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE transactions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    external_id   TEXT    NOT NULL,
    date          TEXT    NOT NULL,
    amount        REAL    NOT NULL,
    currency      TEXT    NOT NULL,
    description   TEXT    NOT NULL,
    counterparty  TEXT,
    balance_after REAL,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(external_id, account_id)
);

CREATE INDEX idx_transactions_account_date ON transactions(account_id, date);

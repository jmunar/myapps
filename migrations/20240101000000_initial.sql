CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS accounts (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_name          TEXT    NOT NULL,
    iban               TEXT,
    enable_banking_id  TEXT    NOT NULL,
    access_token_enc   BLOB    NOT NULL,
    token_expires_at   TEXT    NOT NULL,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS transactions (
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

CREATE TABLE IF NOT EXISTS labels (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name    TEXT    NOT NULL,
    color   TEXT,
    UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS label_rules (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    label_id INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    field    TEXT    NOT NULL,
    pattern  TEXT    NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS transaction_labels (
    transaction_id INTEGER NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    label_id       INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    source         TEXT    NOT NULL DEFAULT 'manual',
    PRIMARY KEY (transaction_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_transactions_account_date ON transactions(account_id, date);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

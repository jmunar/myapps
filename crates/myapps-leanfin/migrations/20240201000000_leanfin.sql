-- LeanFin tables

CREATE TABLE leanfin_accounts (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_name          TEXT    NOT NULL,
    bank_country       TEXT    NOT NULL,
    iban               TEXT,
    session_id         TEXT    NOT NULL,
    account_uid        TEXT    NOT NULL UNIQUE,
    session_expires_at TEXT    NOT NULL,
    balance_amount     REAL,
    balance_currency   TEXT,
    account_type       TEXT NOT NULL DEFAULT 'bank',
    account_name       TEXT,
    asset_category     TEXT,
    archived           INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE leanfin_balance_snapshots (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   INTEGER NOT NULL REFERENCES leanfin_accounts(id) ON DELETE CASCADE,
    timestamp    TEXT    NOT NULL,
    date         TEXT    NOT NULL,
    balance      REAL    NOT NULL,
    balance_type TEXT    NOT NULL DEFAULT 'MANUAL',
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(account_id, balance_type, timestamp)
);

CREATE TABLE leanfin_transactions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL REFERENCES leanfin_accounts(id) ON DELETE CASCADE,
    external_id   TEXT    NOT NULL,
    date          TEXT    NOT NULL,
    amount        REAL    NOT NULL,
    currency      TEXT    NOT NULL,
    description   TEXT    NOT NULL,
    counterparty  TEXT,
    balance_after REAL,
    snapshot_id   INTEGER REFERENCES leanfin_balance_snapshots(id) ON DELETE SET NULL,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(external_id, account_id)
);

CREATE TABLE leanfin_labels (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name    TEXT    NOT NULL,
    color   TEXT,
    UNIQUE(user_id, name)
);

CREATE TABLE leanfin_label_rules (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    label_id INTEGER NOT NULL REFERENCES leanfin_labels(id) ON DELETE CASCADE,
    field    TEXT    NOT NULL,
    pattern  TEXT    NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE leanfin_allocations (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id INTEGER NOT NULL REFERENCES leanfin_transactions(id) ON DELETE CASCADE,
    label_id       INTEGER NOT NULL REFERENCES leanfin_labels(id) ON DELETE CASCADE,
    amount         REAL    NOT NULL,
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE leanfin_pending_links (
    state              TEXT    PRIMARY KEY,
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_name          TEXT    NOT NULL,
    country            TEXT    NOT NULL,
    reauth_account_id  INTEGER REFERENCES leanfin_accounts(id) ON DELETE SET NULL,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE leanfin_api_payloads (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER     REFERENCES leanfin_accounts(id) ON DELETE SET NULL,
    provider      TEXT    NOT NULL DEFAULT 'enable_banking',
    method        TEXT    NOT NULL,
    endpoint      TEXT    NOT NULL,
    request_body  TEXT,
    response_body TEXT,
    status_code   INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    created_at    DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE leanfin_user_settings (
    user_id               INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    enable_banking_app_id TEXT,
    enable_banking_key    BLOB,
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX idx_transactions_account_date ON leanfin_transactions(account_id, date);
CREATE INDEX idx_allocations_transaction ON leanfin_allocations(transaction_id);
CREATE INDEX idx_allocations_label ON leanfin_allocations(label_id);
CREATE INDEX idx_api_payloads_account ON leanfin_api_payloads(account_id);
CREATE INDEX idx_api_payloads_created ON leanfin_api_payloads(created_at);
CREATE INDEX idx_transactions_snapshot ON leanfin_transactions(snapshot_id);

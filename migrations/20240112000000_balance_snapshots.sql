-- Rename daily_balances → balance_snapshots with timestamp + balance_type columns.
-- Keeps a redundant `date` column for efficient index-based filtering.

CREATE TABLE balance_snapshots (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    timestamp    TEXT    NOT NULL,
    date         TEXT    NOT NULL,
    balance      REAL    NOT NULL,
    balance_type TEXT    NOT NULL DEFAULT 'MANUAL',
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(account_id, balance_type, timestamp)
);

-- Migrate existing data (date → midnight UTC timestamp)
INSERT INTO balance_snapshots (account_id, timestamp, date, balance, balance_type, created_at)
SELECT
    db.account_id,
    db.date || 'T23:59:59Z',
    db.date,
    db.balance,
    CASE WHEN a.account_type = 'manual' THEN 'MANUAL' ELSE 'ITAV' END,
    db.created_at
FROM daily_balances db
JOIN accounts a ON a.id = db.account_id;

DROP TABLE daily_balances;

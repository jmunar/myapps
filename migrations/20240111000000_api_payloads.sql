CREATE TABLE api_payloads (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER     REFERENCES accounts(id) ON DELETE SET NULL,
    provider      TEXT    NOT NULL DEFAULT 'enable_banking',
    method        TEXT    NOT NULL,
    endpoint      TEXT    NOT NULL,
    request_body  TEXT,
    response_body TEXT,
    status_code   INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    created_at    DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_api_payloads_account   ON api_payloads(account_id);
CREATE INDEX idx_api_payloads_created   ON api_payloads(created_at);

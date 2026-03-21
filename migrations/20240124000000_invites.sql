CREATE TABLE IF NOT EXISTS invites (
    token      TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

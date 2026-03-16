CREATE TABLE IF NOT EXISTS leanfin_user_settings (
    user_id               INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    enable_banking_app_id TEXT,
    enable_banking_key    BLOB,
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

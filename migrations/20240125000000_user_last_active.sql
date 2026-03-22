-- Track last activity for user cleanup.
ALTER TABLE users ADD COLUMN last_active_at TEXT;

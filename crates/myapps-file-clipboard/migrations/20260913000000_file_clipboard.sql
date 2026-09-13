-- FileClipboard tables
--
-- File *contents* never live in SQLite: a 5 GB upload exceeds SQLite's
-- SQLITE_MAX_LENGTH ceiling (~1 GB by default, 2 GB absolute), and blobs that
-- size would bloat every backup of the shared myapps.db. Rows here are
-- metadata; the bytes live under FILE_CLIPBOARD_DIR/<user_id>/<stored_name>.

CREATE TABLE file_clipboard_files (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Display name, as supplied by the uploader. Never used as a path.
    original_name TEXT NOT NULL,
    -- UUID filename on disk. Unique across all users.
    stored_name   TEXT NOT NULL UNIQUE,
    size_bytes    INTEGER NOT NULL,
    -- Advisory only: echoed back as application/octet-stream, never verbatim.
    content_type  TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    -- Absolute expiry, computed at upload time from the user's retention
    -- setting, so changing that setting later is an explicit re-stamping
    -- rather than a retroactive mass delete.
    expires_at    TEXT NOT NULL
);

CREATE INDEX idx_file_clipboard_files_user ON file_clipboard_files(user_id, created_at DESC);
CREATE INDEX idx_file_clipboard_files_expiry ON file_clipboard_files(expires_at);

CREATE TABLE file_clipboard_settings (
    user_id        INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    retention_days INTEGER NOT NULL
);

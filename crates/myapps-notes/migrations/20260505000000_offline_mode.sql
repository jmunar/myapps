-- Offline mode prerequisites: a stable client-generated identity per note,
-- and an append-only Yjs update log keyed off it.

-- SQLite forbids DEFAULT (expr) on ALTER TABLE ADD COLUMN, so we rebuild the
-- table to attach a default UUID generator to client_uuid.
CREATE TABLE notes_notes_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_uuid TEXT    NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    title       TEXT    NOT NULL DEFAULT '',
    body        TEXT    NOT NULL DEFAULT '',
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO notes_notes_new (id, user_id, client_uuid, title, body, pinned, created_at, updated_at)
SELECT id, user_id, lower(hex(randomblob(16))), title, body, pinned, created_at, updated_at
FROM notes_notes;

DROP TABLE notes_notes;
ALTER TABLE notes_notes_new RENAME TO notes_notes;

CREATE INDEX idx_notes_notes_user ON notes_notes(user_id);
CREATE INDEX idx_notes_notes_updated ON notes_notes(user_id, updated_at DESC);
CREATE UNIQUE INDEX idx_notes_notes_client_uuid ON notes_notes(client_uuid);

-- Append-only Yjs update log: one row per CRDT update message received over the
-- WebSocket sync channel. Compacted into a single snapshot row on idle eviction.
CREATE TABLE notes_note_updates (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id     INTEGER NOT NULL REFERENCES notes_notes(id) ON DELETE CASCADE,
    update_blob BLOB    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_notes_note_updates_note ON notes_note_updates(note_id, id);

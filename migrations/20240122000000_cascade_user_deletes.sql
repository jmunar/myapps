-- Add ON DELETE CASCADE to all user_id foreign keys that were missing it.
-- SQLite cannot ALTER constraints, so we recreate the tables.

PRAGMA foreign_keys = OFF;

-- voice_jobs
CREATE TABLE voice_jobs_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'processing', 'done', 'failed')),
    original_filename TEXT NOT NULL,
    audio_path      TEXT NOT NULL,
    transcription   TEXT,
    error_message   TEXT,
    model_used      TEXT NOT NULL DEFAULT 'base',
    duration_secs   REAL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at    TEXT,
    CONSTRAINT valid_done CHECK (
        status != 'done' OR transcription IS NOT NULL
    )
);
INSERT INTO voice_jobs_new SELECT * FROM voice_jobs;
DROP TABLE voice_jobs;
ALTER TABLE voice_jobs_new RENAME TO voice_jobs;
CREATE INDEX idx_voice_jobs_user_status ON voice_jobs(user_id, status);

-- user_app_visibility
CREATE TABLE user_app_visibility_new (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    app_key  TEXT NOT NULL,
    visible  INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (user_id, app_key)
);
INSERT INTO user_app_visibility_new SELECT * FROM user_app_visibility;
DROP TABLE user_app_visibility;
ALTER TABLE user_app_visibility_new RENAME TO user_app_visibility;

-- classroom_classrooms
CREATE TABLE classroom_classrooms_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label       TEXT    NOT NULL,
    pupils      TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO classroom_classrooms_new SELECT * FROM classroom_classrooms;
DROP TABLE classroom_classrooms;
ALTER TABLE classroom_classrooms_new RENAME TO classroom_classrooms;
CREATE INDEX idx_classroom_classrooms_user ON classroom_classrooms(user_id);

-- classroom_form_types
CREATE TABLE classroom_form_types_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    columns_json TEXT    NOT NULL DEFAULT '[]',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO classroom_form_types_new SELECT * FROM classroom_form_types;
DROP TABLE classroom_form_types;
ALTER TABLE classroom_form_types_new RENAME TO classroom_form_types;
CREATE INDEX idx_classroom_form_types_user ON classroom_form_types(user_id);

-- classroom_inputs
CREATE TABLE classroom_inputs_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    classroom_id INTEGER NOT NULL REFERENCES classroom_classrooms(id) ON DELETE CASCADE,
    form_type_id INTEGER NOT NULL REFERENCES classroom_form_types(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    csv_data     TEXT    NOT NULL DEFAULT '',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO classroom_inputs_new SELECT * FROM classroom_inputs;
DROP TABLE classroom_inputs;
ALTER TABLE classroom_inputs_new RENAME TO classroom_inputs;
CREATE INDEX idx_classroom_inputs_user ON classroom_inputs(user_id);
CREATE INDEX idx_classroom_inputs_classroom ON classroom_inputs(classroom_id);

PRAGMA foreign_keys = ON;

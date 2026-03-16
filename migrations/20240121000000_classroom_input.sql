-- ClassroomInput app tables

CREATE TABLE classroom_classrooms (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id),
    label       TEXT    NOT NULL,
    pupils      TEXT    NOT NULL DEFAULT '',   -- newline-separated pupil names
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_classroom_classrooms_user ON classroom_classrooms(user_id);

CREATE TABLE classroom_form_types (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id),
    name         TEXT    NOT NULL,
    columns_json TEXT    NOT NULL DEFAULT '[]',  -- JSON array: [{"name":"...", "type":"text|number|bool"}]
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_classroom_form_types_user ON classroom_form_types(user_id);

CREATE TABLE classroom_inputs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id),
    classroom_id INTEGER NOT NULL REFERENCES classroom_classrooms(id),
    form_type_id INTEGER NOT NULL REFERENCES classroom_form_types(id),
    name         TEXT    NOT NULL,
    csv_data     TEXT    NOT NULL DEFAULT '',   -- raw CSV: header row + one row per pupil
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_classroom_inputs_user ON classroom_inputs(user_id);
CREATE INDEX idx_classroom_inputs_classroom ON classroom_inputs(classroom_id);

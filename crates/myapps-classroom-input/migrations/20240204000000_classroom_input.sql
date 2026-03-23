-- ClassroomInput tables

CREATE TABLE classroom_input_classrooms (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label       TEXT    NOT NULL,
    pupils      TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE classroom_input_form_types (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    columns_json TEXT    NOT NULL DEFAULT '[]',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE classroom_input_inputs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    classroom_id INTEGER NOT NULL REFERENCES classroom_input_classrooms(id) ON DELETE CASCADE,
    form_type_id INTEGER NOT NULL REFERENCES classroom_input_form_types(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    csv_data     TEXT    NOT NULL DEFAULT '',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_classroom_input_classrooms_user ON classroom_input_classrooms(user_id);
CREATE INDEX idx_classroom_input_form_types_user ON classroom_input_form_types(user_id);
CREATE INDEX idx_classroom_input_inputs_user ON classroom_input_inputs(user_id);
CREATE INDEX idx_classroom_input_inputs_classroom ON classroom_input_inputs(classroom_id);

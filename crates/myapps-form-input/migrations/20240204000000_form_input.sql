-- FormInput tables

CREATE TABLE form_input_row_sets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label       TEXT    NOT NULL,
    rows        TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE form_input_form_types (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    columns_json TEXT    NOT NULL DEFAULT '[]',
    fixed_rows   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE form_input_inputs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    row_set_id   INTEGER          REFERENCES form_input_row_sets(id) ON DELETE CASCADE,
    form_type_id INTEGER NOT NULL REFERENCES form_input_form_types(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    csv_data     TEXT    NOT NULL DEFAULT '',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_form_input_row_sets_user ON form_input_row_sets(user_id);
CREATE INDEX idx_form_input_form_types_user ON form_input_form_types(user_id);
CREATE INDEX idx_form_input_inputs_user ON form_input_inputs(user_id);
CREATE INDEX idx_form_input_inputs_row_set ON form_input_inputs(row_set_id);

-- MindFlow tables

CREATE TABLE mindflow_categories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    color       TEXT    NOT NULL DEFAULT '#6B6B6B',
    icon        TEXT,
    parent_id   INTEGER REFERENCES mindflow_categories(id) ON DELETE SET NULL,
    archived    INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, name)
);

CREATE TABLE mindflow_thoughts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id             INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category_id         INTEGER REFERENCES mindflow_categories(id) ON DELETE SET NULL,
    parent_thought_id   INTEGER REFERENCES mindflow_thoughts(id) ON DELETE SET NULL,
    content             TEXT    NOT NULL,
    status              TEXT    NOT NULL DEFAULT 'active',
    created_at          TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE mindflow_comments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    thought_id  INTEGER NOT NULL REFERENCES mindflow_thoughts(id) ON DELETE CASCADE,
    content     TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE mindflow_actions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    thought_id  INTEGER NOT NULL REFERENCES mindflow_thoughts(id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT    NOT NULL,
    due_date    TEXT,
    priority    TEXT    NOT NULL DEFAULT 'medium',
    status      TEXT    NOT NULL DEFAULT 'pending',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE INDEX idx_mindflow_categories_user ON mindflow_categories(user_id);
CREATE INDEX idx_mindflow_thoughts_user ON mindflow_thoughts(user_id);
CREATE INDEX idx_mindflow_thoughts_category ON mindflow_thoughts(category_id);
CREATE INDEX idx_mindflow_thoughts_status ON mindflow_thoughts(status);
CREATE INDEX idx_mindflow_thoughts_parent ON mindflow_thoughts(parent_thought_id);
CREATE INDEX idx_mindflow_comments_thought ON mindflow_comments(thought_id);
CREATE INDEX idx_mindflow_actions_thought ON mindflow_actions(thought_id);
CREATE INDEX idx_mindflow_actions_user ON mindflow_actions(user_id);
CREATE INDEX idx_mindflow_actions_status ON mindflow_actions(status);

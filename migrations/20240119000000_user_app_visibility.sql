CREATE TABLE user_app_visibility (
    user_id  INTEGER NOT NULL REFERENCES users(id),
    app_key  TEXT NOT NULL,
    visible  INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (user_id, app_key)
);

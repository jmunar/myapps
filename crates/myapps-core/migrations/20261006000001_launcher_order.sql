-- The order a user has dragged the launcher cards into. App keys missing here
-- (apps added since, or a user who never reordered) follow the saved ones in
-- registry order.
CREATE TABLE user_app_order (
    user_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    app_key   TEXT    NOT NULL,
    position  INTEGER NOT NULL,
    PRIMARY KEY (user_id, app_key)
);

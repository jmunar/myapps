-- User language preferences for i18n support.
CREATE TABLE IF NOT EXISTS user_settings (
    user_id  INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    language TEXT NOT NULL DEFAULT 'en'
);

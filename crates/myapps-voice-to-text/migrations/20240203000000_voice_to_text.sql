-- VoiceToText tables

CREATE TABLE voice_to_text_jobs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id             INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status              TEXT NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'processing', 'done', 'failed')),
    original_filename   TEXT NOT NULL,
    audio_path          TEXT NOT NULL,
    transcription       TEXT,
    error_message       TEXT,
    model_used          TEXT NOT NULL DEFAULT 'base',
    duration_secs       REAL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at        TEXT,
    CONSTRAINT valid_done CHECK (
        status != 'done' OR transcription IS NOT NULL
    )
);

CREATE INDEX idx_voice_to_text_jobs_user_status ON voice_to_text_jobs(user_id, status);

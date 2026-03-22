# VoiceToText Database Schema

### voice_to_text_jobs

| Column            | Type    | Notes                                        |
|-------------------|---------|----------------------------------------------|
| id                | INTEGER | PK, autoincrement                            |
| user_id           | INTEGER | FK → users                                   |
| status            | TEXT    | 'pending', 'processing', 'done', 'failed'   |
| original_filename | TEXT    | NOT NULL, user-uploaded filename              |
| audio_path        | TEXT    | NOT NULL, path to stored file on disk         |
| transcription     | TEXT    | Nullable, populated when status = 'done'     |
| error_message     | TEXT    | Nullable, populated when status = 'failed'   |
| model_used        | TEXT    | 'tiny' or 'base', default 'base'            |
| duration_secs     | REAL    | Nullable, processing wall time               |
| created_at        | TEXT    | ISO 8601                                     |
| completed_at      | TEXT    | Nullable, set when processing finishes        |

Check constraint: `status != 'done' OR transcription IS NOT NULL`.

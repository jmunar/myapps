-- Rename voice_* and classroom_* tables so prefixes match app keys exactly.
-- voice_* → voice_to_text_*, classroom_* → classroom_input_*

PRAGMA foreign_keys = OFF;

-- ── VoiceToText ──────────────────────────────────────────────
ALTER TABLE voice_jobs RENAME TO voice_to_text_jobs;
DROP INDEX IF EXISTS idx_voice_jobs_user_status;
CREATE INDEX idx_voice_to_text_jobs_user_status ON voice_to_text_jobs(user_id, status);

-- ── ClassroomInput ───────────────────────────────────────────
ALTER TABLE classroom_classrooms RENAME TO classroom_input_classrooms;
DROP INDEX IF EXISTS idx_classroom_classrooms_user;
CREATE INDEX idx_classroom_input_classrooms_user ON classroom_input_classrooms(user_id);

ALTER TABLE classroom_form_types RENAME TO classroom_input_form_types;
DROP INDEX IF EXISTS idx_classroom_form_types_user;
CREATE INDEX idx_classroom_input_form_types_user ON classroom_input_form_types(user_id);

ALTER TABLE classroom_inputs RENAME TO classroom_input_inputs;
DROP INDEX IF EXISTS idx_classroom_inputs_user;
DROP INDEX IF EXISTS idx_classroom_inputs_classroom;
CREATE INDEX idx_classroom_input_inputs_user ON classroom_input_inputs(user_id);
CREATE INDEX idx_classroom_input_inputs_classroom ON classroom_input_inputs(classroom_id);

PRAGMA foreign_keys = ON;

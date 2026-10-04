-- Challenges tables

-- Problem catalogue, shared by every user and filled by `myapps import`.
-- No user_id, so delete-user-app-data leaves it alone.
CREATE TABLE challenges_problems (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    dataset       TEXT    NOT NULL,          -- Dataset::key(): 'ugphysics' | 'hendrycks-math'
    source_key    TEXT    NOT NULL,          -- stable id within the dataset
    subject       TEXT    NOT NULL,
    topic         TEXT,
    difficulty    INTEGER NOT NULL,          -- 1..=Dataset::max_level()
    source_level  TEXT    NOT NULL,          -- the dataset's own `level`, verbatim
    problem       TEXT    NOT NULL,
    solution      TEXT    NOT NULL,
    answer        TEXT    NOT NULL,
    answer_type   TEXT,                      -- UGPhysics only; for a future grader
    unit          TEXT,
    source_url    TEXT    NOT NULL,
    license       TEXT    NOT NULL,
    imported_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (dataset, source_key)
);

CREATE INDEX idx_challenges_problems_pick
    ON challenges_problems(dataset, subject, difficulty);

-- One row per dataset whose import has run to the end. A dataset with problems
-- but no row here was interrupted, and `serve` imports it again.
CREATE TABLE challenges_imports (
    dataset       TEXT    PRIMARY KEY,
    problems      INTEGER NOT NULL,
    completed_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- No ON DELETE CASCADE on problem_id: the import only upserts, so nothing
-- should delete a problem, and if something does it must fail rather than
-- take the attempt history with it.
CREATE TABLE challenges_attempts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    problem_id  INTEGER NOT NULL REFERENCES challenges_problems(id),
    correct     INTEGER NOT NULL,
    level_at    INTEGER NOT NULL,            -- the user's level in that subject when answered
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_challenges_attempts_user ON challenges_attempts(user_id, problem_id);

-- The staircase state per (user, dataset, subject). Derivable by replaying
-- challenges_attempts, but path-dependent, so it is kept rather than replayed.
CREATE TABLE challenges_progress (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dataset     TEXT    NOT NULL,
    subject     TEXT    NOT NULL,
    level       INTEGER NOT NULL DEFAULT 1,
    streak      INTEGER NOT NULL DEFAULT 0,  -- consecutive correct at this level
    fast_start  INTEGER NOT NULL DEFAULT 1,  -- cleared for good by the first wrong answer
    PRIMARY KEY (user_id, dataset, subject)
);

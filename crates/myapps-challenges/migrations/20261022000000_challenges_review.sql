-- A replay of a past mistake (the Review tab). Recorded like any attempt, so
-- it counts toward clearing or reopening the mistake, but it never moves
-- challenges_progress, the graded draw's ability, or the practice stats.
ALTER TABLE challenges_attempts ADD COLUMN is_review INTEGER NOT NULL DEFAULT 0;

-- The mistake each user is replaying in each dataset, kept for the same
-- reason as challenges_current: a reload brings the same problem back.
CREATE TABLE challenges_review_current (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dataset     TEXT    NOT NULL,
    problem_id  INTEGER NOT NULL REFERENCES challenges_problems(id),
    PRIMARY KEY (user_id, dataset)
);

-- Per-user Challenges settings. No row means the defaults.
CREATE TABLE challenges_settings (
    user_id            INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    review_delay_days  INTEGER NOT NULL DEFAULT 3
                       CHECK (review_delay_days IN (0, 1, 3, 7))
);

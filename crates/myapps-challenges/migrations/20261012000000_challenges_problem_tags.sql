-- Numbers the grader attached to a problem (its difficulty estimate, the
-- answers to the dataset's rubric questions), loaded from the bundle's `tags`.
-- A bundle carries all of its dataset's tags, so a load replaces them. No
-- ON DELETE CASCADE, for the same reason as challenges_attempts: nothing
-- deletes a problem.
CREATE TABLE challenges_problem_tags (
    problem_id  INTEGER NOT NULL REFERENCES challenges_problems(id),
    kind        TEXT    NOT NULL,
    value       REAL    NOT NULL,
    PRIMARY KEY (problem_id, kind)
);

-- The problem each user is working on in each dataset. It stays put until the
-- user marks it or skips it, so leaving, reloading or a server restart brings
-- the same problem back, and the practice page has one URL per dataset rather
-- than one per problem for the browser history to step through.
CREATE TABLE challenges_current (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dataset     TEXT    NOT NULL,
    problem_id  INTEGER NOT NULL REFERENCES challenges_problems(id),
    PRIMARY KEY (user_id, dataset)
);

-- Datasets a user has hidden from the picker. A row means hidden.
CREATE TABLE challenges_hidden_datasets (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    dataset  TEXT    NOT NULL,
    PRIMARY KEY (user_id, dataset)
);

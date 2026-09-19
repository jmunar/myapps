-- Label groups.
--
-- Every label belongs to exactly one group. Group colours are NOT stored: they
-- are derived from the group id in code (`colors::group_color`), so a group's
-- colour can never drift and the user is never asked to pick one. The per-label
-- `color` column goes away with the same change.

CREATE TABLE leanfin_label_groups (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name    TEXT    NOT NULL,
    UNIQUE(user_id, name)
);

-- Every existing user gets the default "Other" group. New users get theirs
-- lazily from `labels::ensure_other_group`.
INSERT INTO leanfin_label_groups (user_id, name)
SELECT id, 'Other' FROM users;

-- Labels point at their group. ON DELETE SET NULL is only a backstop: deleting
-- a group reassigns its labels to "Other" first, and the labels page repairs
-- any stray NULL it finds.
ALTER TABLE leanfin_labels
    ADD COLUMN group_id INTEGER REFERENCES leanfin_label_groups(id) ON DELETE SET NULL;

UPDATE leanfin_labels
SET group_id = (
    SELECT g.id FROM leanfin_label_groups g
    WHERE g.user_id = leanfin_labels.user_id AND g.name = 'Other'
);

ALTER TABLE leanfin_labels DROP COLUMN color;

CREATE INDEX idx_labels_group ON leanfin_labels(group_id);

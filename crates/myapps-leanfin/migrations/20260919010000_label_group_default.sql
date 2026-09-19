-- Groups became renameable, so the default group can no longer be identified by
-- its name. A flag replaces the 'Other' sentinel, and the seeded name changes to
-- "No group" (which reads better next to real group names).
--
-- This is a separate migration rather than an edit to 20260919000000 because
-- that one has already been applied; the migrator runs with ignore_missing, so
-- an edited migration would never re-apply to a deployed database.

ALTER TABLE leanfin_label_groups ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0;

UPDATE leanfin_label_groups SET is_default = 1 WHERE name = 'Other';
UPDATE leanfin_label_groups SET name = 'No group' WHERE is_default = 1;

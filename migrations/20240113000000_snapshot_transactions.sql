-- Link transactions to the balance snapshot where they first appeared.
-- ON DELETE SET NULL: if a snapshot is replaced (same-day dedup), the
-- transactions lose their link but are not deleted.
ALTER TABLE transactions ADD COLUMN snapshot_id INTEGER REFERENCES balance_snapshots(id) ON DELETE SET NULL;
CREATE INDEX idx_transactions_snapshot ON transactions(snapshot_id);

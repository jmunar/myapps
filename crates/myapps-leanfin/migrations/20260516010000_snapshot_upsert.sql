-- Allow record_balance_snapshot to UPSERT same-day rows in place instead of
-- DELETE-then-INSERT, which previously triggered the snapshot_id SET NULL
-- cascade on every transaction linked to the deleted row.
CREATE UNIQUE INDEX idx_snapshots_account_type_date
    ON leanfin_balance_snapshots(account_id, balance_type, date);

-- One-time backfill: re-link transactions orphaned by the old DELETE+INSERT
-- behaviour to the earliest ITAV snapshot on or after their date.
UPDATE leanfin_transactions
SET snapshot_id = (
    SELECT s.id FROM leanfin_balance_snapshots s
    WHERE s.account_id = leanfin_transactions.account_id
      AND s.balance_type = 'ITAV'
      AND s.date >= leanfin_transactions.date
    ORDER BY s.date ASC
    LIMIT 1
)
WHERE snapshot_id IS NULL;

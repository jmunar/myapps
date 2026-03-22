-- Rename LeanFin-specific tables to use leanfin_ prefix,
-- keeping shared platform tables (users, sessions) unchanged.

ALTER TABLE accounts RENAME TO leanfin_accounts;
ALTER TABLE transactions RENAME TO leanfin_transactions;
ALTER TABLE labels RENAME TO leanfin_labels;
ALTER TABLE allocations RENAME TO leanfin_allocations;
ALTER TABLE balance_snapshots RENAME TO leanfin_balance_snapshots;
ALTER TABLE pending_links RENAME TO leanfin_pending_links;
ALTER TABLE api_payloads RENAME TO leanfin_api_payloads;
ALTER TABLE label_rules RENAME TO leanfin_label_rules;

-- Replace transaction_labels with allocations.
-- Each allocation assigns a portion of a transaction's amount to a label.
-- A transaction can have 0..N allocations; their amounts must sum to the
-- transaction amount (enforced in application code, not in the schema).

DROP TABLE IF EXISTS transaction_labels;

CREATE TABLE allocations (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id INTEGER NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    label_id       INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    amount         REAL    NOT NULL,
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX idx_allocations_transaction ON allocations(transaction_id);
CREATE INDEX idx_allocations_label ON allocations(label_id);

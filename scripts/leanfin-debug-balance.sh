#!/usr/bin/env bash
# Debug LeanFin balance discrepancies by querying SQLite directly.
#
# Usage:
#   ./scripts/leanfin-debug-balance.sh <username>                  # list accounts
#   ./scripts/leanfin-debug-balance.sh <username> <account_id>     # dump everything
#   ./scripts/leanfin-debug-balance.sh <username> <account_id> N   # last N transactions (default 50)
#
# Env:
#   DB    path to sqlite db (default: data/myapps.db)

set -euo pipefail

DB="${DB:-data/myapps.db}"

if [[ ! -f "$DB" ]]; then
    echo "error: db not found at $DB" >&2
    exit 1
fi

if [[ $# -lt 1 ]]; then
    sed -n '3,11p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi

USERNAME="$1"
ACCOUNT_ID="${2:-}"
LIMIT="${3:-50}"

sq() { sqlite3 -header -column "$DB" "$@"; }
sq_raw() { sqlite3 "$DB" "$@"; }

USER_ID="$(sq_raw "SELECT id FROM users WHERE username = '$USERNAME';")"
if [[ -z "$USER_ID" ]]; then
    echo "error: user '$USERNAME' not found" >&2
    exit 1
fi
echo "user: $USERNAME (id=$USER_ID)"
echo

if [[ -z "$ACCOUNT_ID" ]]; then
    echo "=== accounts ==="
    sq <<SQL
SELECT id,
       account_type     AS type,
       bank_name        AS bank,
       COALESCE(account_name, '')  AS name,
       COALESCE(iban, '')          AS iban,
       printf('%.2f', COALESCE(balance_amount, 0)) AS balance,
       COALESCE(balance_currency, '')              AS ccy,
       archived
FROM leanfin_accounts
WHERE user_id = $USER_ID
ORDER BY archived ASC, id ASC;
SQL
    echo
    echo "rerun with an account id, e.g.:"
    echo "  $0 $USERNAME <account_id>"
    exit 0
fi

# Verify account belongs to the user
OWNER="$(sq_raw "SELECT user_id FROM leanfin_accounts WHERE id = $ACCOUNT_ID;")"
if [[ "$OWNER" != "$USER_ID" ]]; then
    echo "error: account $ACCOUNT_ID does not belong to user $USERNAME (owner user_id=$OWNER)" >&2
    exit 1
fi

echo "=== account ==="
sq <<SQL
SELECT id, account_type, bank_name, COALESCE(account_name,'') AS name,
       COALESCE(iban,'') AS iban,
       printf('%.2f', COALESCE(balance_amount,0)) AS stored_balance,
       balance_currency AS ccy,
       archived, created_at
FROM leanfin_accounts WHERE id = $ACCOUNT_ID;
SQL
echo

echo "=== balance snapshots (all, oldest first) ==="
sq <<SQL
SELECT id,
       date,
       timestamp,
       balance_type AS type,
       printf('%.2f', balance) AS balance,
       (SELECT COUNT(*) FROM leanfin_transactions t WHERE t.snapshot_id = s.id)        AS linked_txns,
       printf('%.2f', COALESCE(
           (SELECT SUM(amount) FROM leanfin_transactions t WHERE t.snapshot_id = s.id),
           0)) AS linked_sum
FROM leanfin_balance_snapshots s
WHERE account_id = $ACCOUNT_ID
ORDER BY date ASC, id ASC;
SQL
echo

echo "=== reconciliation per consecutive ITAV pair (matches services/balance.rs check_reconciliation) ==="
echo "expected = prev_itav.balance + SUM(txns linked to new snapshot); diff vs new_itav.balance"
sq <<SQL
WITH itav AS (
    SELECT id, date, balance,
           ROW_NUMBER() OVER (ORDER BY date ASC, id ASC) AS rn
    FROM leanfin_balance_snapshots
    WHERE account_id = $ACCOUNT_ID AND balance_type = 'ITAV'
),
pairs AS (
    SELECT a.date AS prev_date, a.balance AS prev_balance,
           b.id   AS new_id,    b.date    AS new_date,    b.balance AS new_balance
    FROM itav a
    JOIN itav b ON b.rn = a.rn + 1
)
SELECT prev_date,
       new_date,
       printf('%.2f', prev_balance) AS prev_balance,
       printf('%.2f', new_balance)  AS new_balance,
       printf('%.2f', COALESCE((SELECT SUM(amount) FROM leanfin_transactions t WHERE t.snapshot_id = pairs.new_id), 0)) AS linked_sum,
       printf('%.2f', prev_balance + COALESCE((SELECT SUM(amount) FROM leanfin_transactions t WHERE t.snapshot_id = pairs.new_id), 0)) AS expected,
       printf('%+.2f', (prev_balance + COALESCE((SELECT SUM(amount) FROM leanfin_transactions t WHERE t.snapshot_id = pairs.new_id), 0)) - new_balance) AS diff
FROM pairs
ORDER BY new_date ASC;
SQL
echo

echo "=== orphan transactions (snapshot_id IS NULL) ==="
sq <<SQL
SELECT COUNT(*) AS count,
       printf('%.2f', COALESCE(SUM(amount),0)) AS sum_amount,
       MIN(date) AS earliest,
       MAX(date) AS latest
FROM leanfin_transactions
WHERE account_id = $ACCOUNT_ID AND snapshot_id IS NULL;
SQL
echo

echo "=== last $LIMIT transactions (newest first), with running balance ==="
echo "running_balance is computed forward from the earliest snapshot; compare against balance_after."
sq <<SQL
WITH first_snap AS (
    SELECT id, date, balance
    FROM leanfin_balance_snapshots
    WHERE account_id = $ACCOUNT_ID
    ORDER BY date ASC, id ASC
    LIMIT 1
),
txns AS (
    SELECT t.id, t.date, t.amount, t.currency, t.description, t.counterparty,
           t.balance_after, t.snapshot_id, t.external_id,
           ROW_NUMBER() OVER (ORDER BY t.date ASC, t.id ASC) AS rn
    FROM leanfin_transactions t
    WHERE t.account_id = $ACCOUNT_ID
),
-- start from the earliest snapshot's balance, but subtract any txns linked to
-- THAT snapshot (they happened BEFORE the snapshot), so the "starting" balance
-- equals what existed before the very first transaction in this account's history.
base AS (
    SELECT COALESCE((SELECT balance FROM first_snap), 0)
         - COALESCE((SELECT SUM(amount) FROM leanfin_transactions
                     WHERE snapshot_id = (SELECT id FROM first_snap)
                       AND date < (SELECT date FROM first_snap)), 0)
         AS start_balance
),
running AS (
    SELECT txns.*,
           (SELECT start_balance FROM base)
             + SUM(amount) OVER (ORDER BY rn ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)
           AS running_balance
    FROM txns
)
SELECT id,
       date,
       printf('%+.2f', amount) AS amount,
       currency AS ccy,
       substr(COALESCE(counterparty,''), 1, 20) AS counterparty,
       substr(description, 1, 40)               AS description,
       printf('%.2f', COALESCE(balance_after, 0)) AS balance_after,
       printf('%.2f', running_balance)           AS running_balance,
       printf('%+.2f', COALESCE(balance_after, running_balance) - running_balance) AS diff_vs_running,
       snapshot_id AS snap
FROM running
ORDER BY date DESC, id DESC
LIMIT $LIMIT;
SQL
echo

echo "=== summary ==="
sq <<SQL
SELECT
    (SELECT printf('%.2f', COALESCE(balance_amount,0)) FROM leanfin_accounts WHERE id = $ACCOUNT_ID) AS stored_account_balance,
    (SELECT printf('%.2f', balance) FROM leanfin_balance_snapshots
     WHERE account_id = $ACCOUNT_ID ORDER BY date DESC, id DESC LIMIT 1)                              AS latest_snapshot_balance,
    (SELECT printf('%.2f', COALESCE(SUM(amount),0)) FROM leanfin_transactions
     WHERE account_id = $ACCOUNT_ID)                                                                  AS sum_of_all_txns,
    (SELECT COUNT(*) FROM leanfin_transactions WHERE account_id = $ACCOUNT_ID)                        AS txn_count;
SQL

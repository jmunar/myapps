//! Auto-labeling rules.
//!
//! Rules never write allocations on their own. A matching rule only *suggests*
//! a label for the full amount of an unallocated transaction; the suggestion is
//! written to `leanfin_allocations` when the user presses "Done" in the
//! allocation editor. That keeps a sync from silently reconciling transactions
//! the user has not looked at yet.

use anyhow::Result;
use sqlx::SqlitePool;

/// One rule, joined with the label it assigns so a suggestion can be rendered
/// without a second query.
#[derive(sqlx::FromRow, Clone)]
pub struct Rule {
    pub label_id: i64,
    pub field: String,
    pub pattern: String,
    pub label_name: String,
    pub group_id: Option<i64>,
}

/// A rule match for one transaction.
#[derive(Clone)]
pub struct Suggestion {
    pub label_id: i64,
    pub label_name: String,
    pub group_id: Option<i64>,
    /// Full absolute amount of the transaction — rules always suggest the whole
    /// transaction, never a split.
    pub amount: f64,
}

/// Load every rule belonging to the user, highest priority first.
pub async fn load_rules(pool: &SqlitePool, user_id: i64) -> Result<Vec<Rule>> {
    let rules = sqlx::query_as::<_, Rule>(
        r#"SELECT lr.label_id, lr.field, lr.pattern, l.name AS label_name, l.group_id
           FROM leanfin_label_rules lr
           JOIN leanfin_labels l ON lr.label_id = l.id
           WHERE l.user_id = ?
           ORDER BY lr.priority DESC, lr.id"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rules)
}

/// First rule matching this transaction, if any. Highest priority wins, which is
/// the order `load_rules` returns.
pub fn match_rule<'a>(
    rules: &'a [Rule],
    description: &str,
    counterparty: Option<&str>,
) -> Option<&'a Rule> {
    rules.iter().find(|rule| {
        let field_value = match rule.field.as_str() {
            "description" => description,
            "counterparty" => match counterparty {
                Some(v) => v,
                None => return false,
            },
            _ => return false,
        };
        field_value
            .to_lowercase()
            .contains(&rule.pattern.to_lowercase())
    })
}

/// Suggestion for a single transaction, or `None` when it already has
/// allocations or no rule matches.
pub async fn suggest_for_transaction(
    pool: &SqlitePool,
    user_id: i64,
    txn_id: i64,
) -> Result<Option<Suggestion>> {
    let txn: Option<(f64, String, Option<String>)> = sqlx::query_as(
        r#"SELECT t.amount, t.description, t.counterparty
           FROM leanfin_transactions t
           JOIN leanfin_accounts a ON t.account_id = a.id
           WHERE t.id = ? AND a.user_id = ?
             AND t.id NOT IN (SELECT transaction_id FROM leanfin_allocations)"#,
    )
    .bind(txn_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some((amount, description, counterparty)) = txn else {
        return Ok(None);
    };

    let rules = load_rules(pool, user_id).await?;
    Ok(
        match_rule(&rules, &description, counterparty.as_deref()).map(|rule| Suggestion {
            label_id: rule.label_id,
            label_name: rule.label_name.clone(),
            group_id: rule.group_id,
            amount: amount.abs(),
        }),
    )
}

/// Write the suggested allocation for a transaction, if one applies. Returns the
/// label that was assigned. A transaction that already has allocations, or that
/// no rule matches, is left untouched.
pub async fn commit_suggestion(
    pool: &SqlitePool,
    user_id: i64,
    txn_id: i64,
) -> Result<Option<String>> {
    let Some(suggestion) = suggest_for_transaction(pool, user_id, txn_id).await? else {
        return Ok(None);
    };

    // A zero-amount transaction has nothing to allocate.
    if suggestion.amount < 0.01 {
        return Ok(None);
    }

    sqlx::query(
        "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
    )
    .bind(txn_id)
    .bind(suggestion.label_id)
    .bind(suggestion.amount)
    .execute(pool)
    .await?;

    Ok(Some(suggestion.label_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(field: &str, pattern: &str, label_id: i64) -> Rule {
        Rule {
            label_id,
            field: field.to_string(),
            pattern: pattern.to_string(),
            label_name: format!("label-{label_id}"),
            group_id: Some(1),
        }
    }

    #[test]
    fn matches_counterparty_case_insensitively() {
        let rules = vec![rule("counterparty", "Mercadona", 7)];
        let hit = match_rule(&rules, "weekly shop", Some("MERCADONA SA"));
        assert_eq!(hit.map(|r| r.label_id), Some(7));
    }

    #[test]
    fn matches_description_when_counterparty_is_absent() {
        let rules = vec![rule("description", "rent", 3)];
        assert_eq!(
            match_rule(&rules, "Monthly RENT", None).map(|r| r.label_id),
            Some(3)
        );
    }

    #[test]
    fn counterparty_rule_skips_transactions_without_one() {
        let rules = vec![rule("counterparty", "netflix", 1)];
        assert!(match_rule(&rules, "netflix subscription", None).is_none());
    }

    #[test]
    fn first_rule_in_priority_order_wins() {
        let rules = vec![rule("description", "shop", 1), rule("description", "s", 2)];
        assert_eq!(
            match_rule(&rules, "corner shop", None).map(|r| r.label_id),
            Some(1)
        );
    }

    #[test]
    fn unknown_field_never_matches() {
        let rules = vec![rule("iban", "ES", 4)];
        assert!(match_rule(&rules, "ES123", Some("ES123")).is_none());
    }
}

//! Cross-user reachability of the routes this branch added.
//!
//! Every one of these handlers is guarded by a `user_id` predicate in SQL and
//! nothing else, so each route needs a *real* second user's row aimed at it —
//! a made-up id like 99999 passes a missing-ownership check just as happily as
//! a correct one.
//!
//! The harness keeps one cookie jar, so `become_mallory` simply logs in as
//! someone else: every request after it is the attacker's.

use myapps_leanfin::LeanFinApp;
use myapps_test_harness::TestApp;

/// Rows belonging to `seeduser` that a second user will try to reach.
struct Victim {
    label: i64,
    group: i64,
    rule: i64,
    account: i64,
    txn: i64,
}

/// Seed `seeduser`, note the rows worth stealing, then log in as `mallory`.
async fn seed_then_become_mallory(app: &TestApp) -> Victim {
    app.seed_and_login(&LeanFinApp).await;

    let label: i64 = sqlx::query_scalar("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let group: i64 =
        sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE name = 'Essentials'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let rule: i64 = sqlx::query_scalar("SELECT id FROM leanfin_label_rules WHERE label_id = ?")
        .bind(label)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let account: i64 =
        sqlx::query_scalar("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // An unallocated transaction the seeded Mercadona rule matches, so a
    // "Done" from anyone would have something to commit.
    let txn: i64 = sqlx::query_scalar(
        "SELECT id FROM leanfin_transactions WHERE counterparty = 'Mercadona' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM leanfin_allocations WHERE transaction_id = ?")
        .bind(txn)
        .execute(&app.pool)
        .await
        .unwrap();

    app.login_as("mallory", "mallory-pass").await;

    Victim {
        label,
        group,
        rule,
        account,
        txn,
    }
}

// ── Detail frames ────────────────────────────────────────────

#[tokio::test]
async fn label_panel_of_another_user_renders_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .get(&format!("/leanfin/labels/{}/panel", victim.label))
        .await
        .text();

    assert!(body.is_empty(), "leaked another user's label panel: {body}");
}

#[tokio::test]
async fn group_panel_of_another_user_renders_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .get(&format!("/leanfin/label-groups/{}/panel", victim.group))
        .await
        .text();

    assert!(body.is_empty(), "leaked another user's group panel: {body}");
}

// ── Group mutations ──────────────────────────────────────────

#[tokio::test]
async fn renaming_another_users_group_leaves_it_unchanged() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    app.server
        .post(&format!("/leanfin/label-groups/{}/edit", victim.group))
        .form(&serde_json::json!({"name": "Pwned"}))
        .expect_failure()
        .await;

    let name: String = sqlx::query_scalar("SELECT name FROM leanfin_label_groups WHERE id = ?")
        .bind(victim.group)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name, "Essentials");
}

#[tokio::test]
async fn deleting_another_users_group_keeps_it_and_its_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    app.server
        .post(&format!("/leanfin/label-groups/{}/delete", victim.group))
        .expect_failure()
        .await;

    let still_there: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_groups WHERE id = ?")
            .bind(victim.group)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(still_there, 1, "another user's group was deleted");

    // Nor may its labels be swept into the attacker's default group.
    let group_of_label: i64 =
        sqlx::query_scalar("SELECT group_id FROM leanfin_labels WHERE id = ?")
            .bind(victim.label)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(group_of_label, victim.group);
}

// ── Label mutations ──────────────────────────────────────────

#[tokio::test]
async fn renaming_another_users_label_leaves_it_unchanged() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    app.server
        .post(&format!("/leanfin/labels/{}/edit", victim.label))
        .form(&serde_json::json!({"name": "Pwned"}))
        .expect_failure()
        .await;

    let name: String = sqlx::query_scalar("SELECT name FROM leanfin_labels WHERE id = ?")
        .bind(victim.label)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name, "Groceries");
}

#[tokio::test]
async fn deleting_another_users_label_keeps_it() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    app.server
        .post(&format!("/leanfin/labels/{}/delete", victim.label))
        .expect_failure()
        .await;

    let still_there: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_labels WHERE id = ?")
        .bind(victim.label)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(still_there, 1, "another user's label was deleted");
}

#[tokio::test]
async fn moving_another_users_label_leaves_its_group_alone() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    // Mallory needs a group of her own to move the label into.
    app.server
        .post("/leanfin/label-groups/create")
        .form(&serde_json::json!({"name": "Mine"}))
        .expect_failure()
        .await;
    let mine: i64 = sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE name = 'Mine'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{}/group", victim.label))
        .form(&serde_json::json!({"group_id": mine}))
        .expect_failure()
        .await;

    let group_of_label: i64 =
        sqlx::query_scalar("SELECT group_id FROM leanfin_labels WHERE id = ?")
            .bind(victim.label)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(group_of_label, victim.group);
}

#[tokio::test]
async fn creating_a_label_in_another_users_group_falls_back_to_the_default() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    app.server
        .post("/leanfin/labels/create")
        .form(&serde_json::json!({"name": "Smuggled", "group_id": victim.group}))
        .expect_failure()
        .await;

    let (group_id, is_default): (i64, bool) = sqlx::query_as(
        r#"SELECT g.id, g.is_default FROM leanfin_labels l
           JOIN leanfin_label_groups g ON l.group_id = g.id
           WHERE l.name = 'Smuggled'"#,
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_ne!(
        group_id, victim.group,
        "label landed in another user's group"
    );
    assert!(is_default, "it should fall back to Mallory's default group");
}

// ── Rules ────────────────────────────────────────────────────

#[tokio::test]
async fn listing_another_users_rules_renders_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .get(&format!("/leanfin/labels/{}/rules", victim.label))
        .await
        .text();

    assert!(body.is_empty(), "leaked another user's rules: {body}");
}

#[tokio::test]
async fn creating_a_rule_on_another_users_label_creates_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_rules WHERE label_id = ?")
            .bind(victim.label)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let body = app
        .server
        .post(&format!("/leanfin/labels/{}/rules/create", victim.label))
        .form(&serde_json::json!({"field": "counterparty", "pattern": "Lidl"}))
        .await
        .text();

    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_rules WHERE label_id = ?")
            .bind(victim.label)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    assert_eq!(
        after, before,
        "a rule was grafted onto another user's label"
    );
    assert!(body.is_empty(), "leaked another user's rules: {body}");
}

#[tokio::test]
async fn deleting_a_rule_on_another_users_label_neither_deletes_nor_leaks() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .post(&format!(
            "/leanfin/labels/{}/rules/{}/delete",
            victim.label, victim.rule
        ))
        .await
        .text();

    let still_there: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_rules WHERE id = ?")
            .bind(victim.rule)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(still_there, 1, "another user's rule was deleted");

    // The handler re-renders the rules panel after deleting. Rendering it for a
    // label the caller does not own would hand back the victim's patterns.
    assert!(
        !body.contains("Mercadona") && !body.contains("Carrefour"),
        "the rules panel leaked another user's patterns: {body}"
    );
}

// ── Transactions ─────────────────────────────────────────────

#[tokio::test]
async fn done_on_another_users_transaction_allocates_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .post(&format!("/leanfin/transactions/{}/done", victim.txn))
        .await
        .text();

    assert!(body.is_empty(), "leaked another user's row: {body}");

    let allocations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_allocations WHERE transaction_id = ?")
            .bind(victim.txn)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(allocations, 0, "Done committed another user's suggestion");
}

#[tokio::test]
async fn allocation_editor_for_another_users_transaction_renders_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .get(&format!("/leanfin/transactions/{}/allocations", victim.txn))
        .await
        .text();

    assert!(body.is_empty(), "leaked another user's editor: {body}");
}

// ── Charts ───────────────────────────────────────────────────

#[tokio::test]
async fn breakdown_chart_for_another_users_group_reports_not_found() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &victim.group.to_string())
        .add_query_param("days", "365")
        .await
        .text();

    assert!(body.contains("showBreakdownEmpty("));
    assert!(body.contains("Group not found."));
    // None of the victim's categories may appear in the payload.
    assert!(!body.contains("Groceries"), "leaked the group's labels");
}

#[tokio::test]
async fn balance_deep_link_to_another_users_account_falls_back_to_all_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let victim = seed_then_become_mallory(&app).await;

    // Mallory needs at least one account of her own, or the page short-circuits
    // to its "no accounts" empty state and proves nothing.
    sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid,
             session_expires_at, account_type, account_name, balance_currency)
         VALUES ((SELECT id FROM users WHERE username = 'mallory'), 'Mallory Bank', '', '',
                 'mallory-uid', '9999-12-31T00:00:00Z', 'manual', 'Mallory Bank', 'EUR')",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let body = app
        .server
        .get("/leanfin/balance-evolution")
        .add_query_param("account_id", &victim.account.to_string())
        .await
        .text();

    assert!(
        body.contains(r#"<option value="" selected>All accounts</option>"#),
        "a foreign account id must fall back to the aggregate view"
    );
    assert!(
        !body.contains(&format!(r#"<option value="{}""#, victim.account)),
        "another user's account appeared in the picker"
    );
    assert!(
        body.contains("account_id=&days=90"),
        "the initial data request must carry no account filter"
    );
}

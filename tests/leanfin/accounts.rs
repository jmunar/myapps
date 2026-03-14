use crate::harness;

// ── Archive feature tests ────────────────────────────────────────

#[tokio::test]
async fn archive_button_shown_for_active_accounts() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Archive"), "missing Archive button");
}

#[tokio::test]
async fn archive_bank_account_hides_from_list() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Allocate all transactions for Santander so archiving is allowed
    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Get the label id for allocations
    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE user_id = 1 LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Allocate all unallocated transactions for this account
    let unallocated: Vec<(i64, f64)> = sqlx::query_as(
        r#"SELECT t.id, t.amount FROM leanfin_transactions t
           WHERE t.account_id = ?
             AND t.id NOT IN (
               SELECT al.transaction_id FROM leanfin_allocations al
               GROUP BY al.transaction_id
               HAVING ABS(SUM(al.amount) - ABS(
                   (SELECT t2.amount FROM leanfin_transactions t2 WHERE t2.id = al.transaction_id)
               )) < 0.01
             )"#,
    )
    .bind(account_id)
    .fetch_all(&app.pool)
    .await
    .unwrap();

    for (txn_id, amount) in &unallocated {
        sqlx::query(
            "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
        )
        .bind(txn_id)
        .bind(label_id)
        .bind(amount.abs())
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let response = app
        .server
        .post(&format!("/leanfin/accounts/{account_id}/archive"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    // Default list should NOT show archived account
    let list = app.server.get("/leanfin/accounts").await;
    let body = list.text();
    assert!(
        !body.contains("Santander"),
        "archived account still shown in default view"
    );
}

#[tokio::test]
async fn archived_accounts_shown_with_toggle() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Archive directly via DB for simplicity
    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE bank_name = 'Santander'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get("/leanfin/accounts")
        .add_query_param("show_archived", "1")
        .await;
    let body = response.text();
    assert!(
        body.contains("Santander"),
        "archived account not shown with toggle"
    );
    assert!(body.contains("Archived"), "missing Archived badge");
    assert!(
        body.contains("account-archived"),
        "missing archived CSS class"
    );
}

#[tokio::test]
async fn show_archived_checkbox_visible_when_archived_exist() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE bank_name = 'Santander'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("Show archived"),
        "missing show archived checkbox"
    );
}

#[tokio::test]
async fn show_archived_checkbox_hidden_when_none_archived() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Unarchive all accounts so none are archived
    sqlx::query("UPDATE leanfin_accounts SET archived = 0")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        !body.contains("Show archived"),
        "show archived checkbox should not appear when no archived accounts"
    );
}

#[tokio::test]
async fn archive_blocked_with_unallocated_transactions() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // The seed data has some unallocated transactions, so archiving should fail
    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Ensure there are unallocated transactions
    sqlx::query("DELETE FROM leanfin_allocations WHERE transaction_id IN (SELECT id FROM leanfin_transactions WHERE account_id = ? LIMIT 1)")
        .bind(account_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/{account_id}/archive"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    // Should redirect with error and account should still be visible
    let location = response.header("location").to_str().unwrap().to_string();
    assert!(
        location.contains("archive_error"),
        "missing archive_error in redirect"
    );
}

#[tokio::test]
async fn archive_error_shows_banner() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/accounts")
        .add_query_param("archive_error", account_id.to_string())
        .await;
    let body = response.text();
    assert!(body.contains("Cannot archive"), "missing error banner");
    assert!(body.contains("unallocated"), "error banner missing details");
}

#[tokio::test]
async fn unarchive_restores_account() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ?")
        .bind(account_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/{account_id}/unarchive"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/leanfin/accounts").await;
    let body = list.text();
    assert!(body.contains("Santander"), "unarchived account not shown");
}

#[tokio::test]
async fn archived_manual_account_edit_redirects() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ?")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    // Edit form should redirect (account not found because archived)
    let response = app
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/edit"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn archived_manual_account_value_redirects() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ?")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/value"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn archived_account_excluded_from_balance_dropdown() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE bank_name = 'Santander'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        !body.contains("Santander"),
        "archived account should not appear in balance dropdown"
    );
    // Other accounts should still be there
    assert!(
        body.contains("ING Direct"),
        "active account missing from dropdown"
    );
}

#[tokio::test]
async fn archived_account_not_synced() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Archive all bank accounts
    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE account_type = 'bank'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.post("/leanfin/sync").await;
    let body = response.text();
    // With all bank accounts archived, sync should report no accounts to sync
    assert!(
        body.contains("No accounts to sync"),
        "archived accounts should not be synced"
    );
}

// ── Existing tests ──────────────────────────────────────────────

#[tokio::test]
async fn accounts_page_renders_linked_accounts() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Santander"));
    assert!(body.contains("ING Direct"));
}

#[tokio::test]
async fn accounts_page_shows_balance_when_present() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    sqlx::query("UPDATE leanfin_accounts SET balance_amount = ?, balance_currency = ? WHERE bank_name = 'Santander'")
        .bind(1234.56_f64)
        .bind("EUR")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("1234.56 EUR"));
    assert!(body.contains(r#"class="account-balance positive""#));
}

#[tokio::test]
async fn accounts_page_shows_negative_balance() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    sqlx::query("UPDATE leanfin_accounts SET balance_amount = ?, balance_currency = ? WHERE bank_name = 'Santander'")
        .bind(-500.00_f64)
        .bind("EUR")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("-500.00"));
    assert!(body.contains(r#"class="account-balance negative""#));
}

#[tokio::test]
async fn accounts_page_hides_balance_when_null() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Clear balances so we can test the null case
    sqlx::query("UPDATE leanfin_accounts SET balance_amount = NULL, balance_currency = NULL")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(!body.contains("account-balance"));
}

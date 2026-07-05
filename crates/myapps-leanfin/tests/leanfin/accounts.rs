// ── Archive feature tests ────────────────────────────────────────

#[tokio::test]
async fn archive_button_shown_for_active_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Archive"), "missing Archive button");
}

#[tokio::test]
async fn archive_bank_account_hides_from_list() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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

// ── Re-authorization tests ──────────────────────────────────────

/// Regression for BUG-90: re-authorizing a bank with two accounts must keep each
/// account's own IBAN and refresh *both* siblings' sessions. The old code matched
/// on the rotating `account_uid` (never matches on re-auth) and then copied
/// `session.accounts.first()`'s uid+IBAN onto only the single reauthed row — which
/// collapsed both accounts onto the same IBAN and left the sibling expired.
#[tokio::test]
async fn reauth_keeps_ibans_distinct_and_refreshes_both_siblings() {
    use myapps_leanfin::services::enable_banking::{self, AccountId, SessionAccount};

    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    const IBAN_A: &str = "ES11 1111 1111 1111 1111 1111";
    const IBAN_B: &str = "ES22 2222 2222 2222 2222 2222";

    // Two accounts from the SAME bank, both with expired sessions and stale UIDs.
    for (iban, uid) in [(IBAN_A, "old_uid_a"), (IBAN_B, "old_uid_b")] {
        sqlx::query(
            "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, account_type) VALUES (1, 'TwinBank', 'ES', ?, 'old_sess', ?, '2020-01-01T00:00:00Z', 'bank')",
        )
        .bind(iban)
        .bind(uid)
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let (reauth_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts WHERE iban = ?")
        .bind(IBAN_A)
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // The bank returns both accounts with FRESH (rotated) UIDs. Order is reversed
    // vs. the DB to exercise the previous "first account wins" bug.
    let new_expiry = (chrono::Utc::now() + chrono::Duration::days(90)).naive_utc();
    let session_accounts = vec![
        SessionAccount {
            uid: "new_uid_b".into(),
            account_id: Some(AccountId {
                iban: Some(IBAN_B.into()),
            }),
        },
        SessionAccount {
            uid: "new_uid_a".into(),
            account_id: Some(AccountId {
                iban: Some(IBAN_A.into()),
            }),
        },
    ];

    let updated = enable_banking::apply_reauth_session(
        &app.pool,
        1,
        reauth_id,
        "new_sess",
        new_expiry,
        &session_accounts,
    )
    .await;
    assert_eq!(updated, 2, "both sibling accounts should be refreshed");

    // Each account keeps its own IBAN and gets the matching fresh UID + session.
    let rows: Vec<(String, String, String, chrono::NaiveDateTime)> = sqlx::query_as(
        "SELECT iban, account_uid, session_id, session_expires_at FROM leanfin_accounts WHERE bank_name = 'TwinBank' ORDER BY iban",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();

    let now = chrono::Utc::now().naive_utc();
    assert_eq!(rows.len(), 2);
    let (iban_a, uid_a, sess_a, exp_a) = &rows[0];
    let (iban_b, uid_b, sess_b, exp_b) = &rows[1];

    assert_eq!(iban_a, IBAN_A, "account A must keep its own IBAN");
    assert_eq!(iban_b, IBAN_B, "account B must keep its own IBAN");
    assert_ne!(iban_a, iban_b, "the two accounts must not share an IBAN");
    assert_eq!(
        uid_a, "new_uid_a",
        "account A should get its matched fresh UID"
    );
    assert_eq!(
        uid_b, "new_uid_b",
        "account B should get its matched fresh UID"
    );
    assert_eq!(sess_a, "new_sess");
    assert_eq!(sess_b, "new_sess");
    assert!(*exp_a > now, "account A should no longer be expired");
    assert!(
        *exp_b > now,
        "account B (sibling) should no longer be expired"
    );
}

/// When an existing row has no stored IBAN to match on, the fallback should still
/// re-authorize the reauthed account (and fill in the IBAN) without erroring.
#[tokio::test]
async fn reauth_fallback_updates_account_without_stored_iban() {
    use myapps_leanfin::services::enable_banking::{self, AccountId, SessionAccount};

    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, account_type) VALUES (1, 'NoIbanBank', 'ES', NULL, 'old_sess', 'old_uid', '2020-01-01T00:00:00Z', 'bank')",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let (reauth_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'NoIbanBank'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let new_expiry = (chrono::Utc::now() + chrono::Duration::days(90)).naive_utc();
    let session_accounts = vec![SessionAccount {
        uid: "fresh_uid".into(),
        account_id: Some(AccountId {
            iban: Some("ES99 9999 9999 9999 9999 9999".into()),
        }),
    }];

    let updated = enable_banking::apply_reauth_session(
        &app.pool,
        1,
        reauth_id,
        "new_sess",
        new_expiry,
        &session_accounts,
    )
    .await;
    assert_eq!(updated, 1);

    let (iban, uid, exp): (Option<String>, String, chrono::NaiveDateTime) = sqlx::query_as(
        "SELECT iban, account_uid, session_expires_at FROM leanfin_accounts WHERE id = ?",
    )
    .bind(reauth_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(iban.as_deref(), Some("ES99 9999 9999 9999 9999 9999"));
    assert_eq!(uid, "fresh_uid");
    assert!(exp > chrono::Utc::now().naive_utc());
}

// ── Account coloring tests ──────────────────────────────────────

#[tokio::test]
async fn update_account_color_persists() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/{account_id}/color"))
        .form(&serde_json::json!({"color": "#ff5733"}))
        .await;
    assert_eq!(response.status_code(), 200);

    // Verify the color was persisted in the database
    let (color,): (Option<String>,) =
        sqlx::query_as("SELECT color FROM leanfin_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(color.as_deref(), Some("#ff5733"));
}

#[tokio::test]
async fn accounts_page_shows_color_stripe_and_picker() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("account-color-stripe"),
        "missing account-color-stripe element"
    );
    assert!(
        body.contains("account-color-picker"),
        "missing account-color-picker element"
    );
}

#[tokio::test]
async fn accounts_page_shows_custom_color_in_style() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Set a specific color on an account
    sqlx::query("UPDATE leanfin_accounts SET color = '#abcdef' WHERE bank_name = 'Santander'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("--account-color:#abcdef"),
        "account item should include --account-color CSS variable"
    );
    assert!(
        body.contains(r##"value="#abcdef""##),
        "color picker should have the account color as value"
    );
}

// ── Existing tests ──────────────────────────────────────────────

#[tokio::test]
async fn accounts_page_renders_linked_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Santander"));
    assert!(body.contains("ING Direct"));
}

#[tokio::test]
async fn accounts_page_shows_balance_when_present() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

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
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Clear balances so we can test the null case
    sqlx::query("UPDATE leanfin_accounts SET balance_amount = NULL, balance_currency = NULL")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(!body.contains("account-balance"));
}

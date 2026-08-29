//! Indexa Capital provider: settings, account list section and link flow.
//!
//! These exercise the routes without touching the live Indexa API — the link
//! form redirects to settings when no token is stored, which is the state a
//! freshly seeded user is in.

#[tokio::test]
async fn accounts_page_shows_indexa_section() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("Indexa Capital"),
        "missing Indexa Capital heading"
    );
    assert!(
        body.contains("No Indexa accounts linked yet."),
        "missing Indexa empty state"
    );
}

#[tokio::test]
async fn accounts_page_prompts_for_token_when_unconfigured() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    // Without a stored token the section offers settings, not the link flow.
    assert!(
        body.contains("Configure Indexa token"),
        "missing configure-token call to action"
    );
    assert!(
        !body.contains("/leanfin/accounts/indexa/link"),
        "link button should be hidden until a token is stored"
    );
}

#[tokio::test]
async fn settings_page_has_indexa_token_field() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/settings").await;
    let body = response.text();
    assert!(
        body.contains(r#"name="indexa_token""#),
        "missing indexa_token field"
    );
    assert!(
        body.contains("Indexa Capital API token"),
        "missing Indexa token label"
    );
    assert!(
        body.contains(r#"type="password""#),
        "token field must not render its value in plain text"
    );
}

#[tokio::test]
async fn indexa_link_form_redirects_to_settings_without_token() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/accounts/indexa/link")
        .expect_failure() // 303 redirect
        .await;
    response.assert_status(axum::http::StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .expect("missing redirect location")
        .to_str()
        .unwrap();
    assert!(
        location.ends_with("/leanfin/settings"),
        "expected redirect to settings, got {location}"
    );
}

#[tokio::test]
async fn indexa_link_submit_redirects_to_settings_without_token() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .post("/leanfin/accounts/indexa/link")
        .form(&serde_json::json!({ "acct_ABC1DE2F": "1" }))
        .expect_failure() // 303 redirect
        .await;
    response.assert_status(axum::http::StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn indexa_account_appears_in_balance_evolution_selector() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'seeduser'")
        .fetch_one(&app.pool)
        .await
        .expect("seeded user not found");

    sqlx::query(
        r#"INSERT INTO leanfin_accounts
           (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at,
            account_type, account_name, asset_category, balance_amount, balance_currency)
           VALUES (?, 'Indexa Capital', 'ES', '', 'ABC1DE2F', '9999-12-31T00:00:00Z',
                   'indexa', 'Indexa ABC1DE2F', 'investment', 1234.56, 'EUR')"#,
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .expect("failed to insert Indexa account");

    let accounts = app.server.get("/leanfin/accounts").await.text();
    assert!(
        accounts.contains("Indexa ABC1DE2F"),
        "Indexa account missing from accounts page"
    );
    assert!(
        accounts.contains("1234.56 EUR"),
        "Indexa balance missing from accounts page"
    );

    // Indexa accounts must be selectable on the balance chart, which is the
    // whole point of syncing a valuation the user cannot get over PSD2.
    let evolution = app.server.get("/leanfin/balance-evolution").await.text();
    assert!(
        evolution.contains("Indexa ABC1DE2F"),
        "Indexa account missing from balance evolution selector"
    );
}

//! Regression coverage for HTML escaping of user-controlled strings.
//!
//! LeanFin rendered account and label names straight into `format!` templates,
//! so a name containing markup executed script on the accounts list, the
//! dashboard and balance-evolution `<option>` lists, and the expenses label
//! pills. Every render site must escape.

#[tokio::test]
async fn user_controlled_names_are_html_escaped() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'seeduser'")
        .fetch_one(&app.pool)
        .await
        .expect("seeded user not found");

    let payload = r#"<img src=x onerror="window.__xss=1">"#;

    sqlx::query(
        r#"INSERT INTO leanfin_accounts
           (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at,
            account_type, account_name, asset_category, balance_amount, balance_currency)
           VALUES (?, ?, 'ES', '', 'xss-acct', '9999-12-31T00:00:00Z',
                   'manual', ?, 'other', 1.0, 'EUR')"#,
    )
    .bind(user_id)
    .bind(payload)
    .bind(payload)
    .execute(&app.pool)
    .await
    .expect("failed to insert account");

    sqlx::query("INSERT INTO leanfin_labels (user_id, name, color) VALUES (?, ?, '#ff0000')")
        .bind(user_id)
        .bind(payload)
        .execute(&app.pool)
        .await
        .expect("failed to insert label");

    for route in [
        "/leanfin",
        "/leanfin/accounts",
        "/leanfin/balance-evolution",
        "/leanfin/expenses",
        "/leanfin/labels",
        "/leanfin/transactions",
    ] {
        let body = app.server.get(route).await.text();
        assert!(
            !body.contains("<img src=x"),
            "{route} rendered an unescaped payload"
        );
        assert!(
            !body.contains(r#"onerror="window.__xss=1""#),
            "{route} rendered an unescaped event handler"
        );
    }
}

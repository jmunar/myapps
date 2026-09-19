//! Regression coverage for HTML escaping of user-controlled strings.
//!
//! LeanFin rendered account, label and group names straight into `format!`
//! templates, so a name containing markup executed script on the accounts list,
//! the dashboard and balance-evolution `<option>` lists, and the breakdown
//! group pills. Every render site must escape — including the JSON payload the
//! breakdown chart embeds in a `<script>` body.

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

    // A group whose name is markup, holding a label whose name is markup: both
    // reach the labels page, the group pills and the chart's JSON payload.
    let group_id: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_label_groups (user_id, name) VALUES (?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(payload)
    .fetch_one(&app.pool)
    .await
    .expect("failed to insert group");

    sqlx::query("INSERT INTO leanfin_labels (user_id, name, group_id) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(payload)
        .bind(group_id)
        .execute(&app.pool)
        .await
        .expect("failed to insert label");

    for route in [
        "/leanfin",
        "/leanfin/accounts",
        "/leanfin/balance-evolution",
        "/leanfin/breakdown",
        "/leanfin/labels",
        "/leanfin/transactions",
        &format!("/leanfin/breakdown/chart?group_id={group_id}&days=90"),
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

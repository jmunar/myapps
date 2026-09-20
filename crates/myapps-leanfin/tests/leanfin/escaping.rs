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

/// A rule's field and pattern reach the rules panel, which is rendered on the
/// label panel, on `GET /labels/{id}/rules` and again after every rule create
/// or delete. They were interpolated raw until this branch.
#[tokio::test]
async fn rule_patterns_are_html_escaped_everywhere_the_panel_is_rendered() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let label: i64 = sqlx::query_scalar("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let payload = r#"<img src=x onerror="window.__xss=1">"#;

    // Through the form, as a user would.
    let created = app
        .server
        .post(&format!("/leanfin/labels/{label}/rules/create"))
        .form(&serde_json::json!({"field": "counterparty", "pattern": payload}))
        .await
        .text();

    let rule: i64 = sqlx::query_scalar(
        "SELECT id FROM leanfin_label_rules WHERE label_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(label)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let listed = app
        .server
        .get(&format!("/leanfin/labels/{label}/rules"))
        .await
        .text();
    let panel = app
        .server
        .get(&format!("/leanfin/labels/{label}/panel"))
        .await
        .text();
    let after_delete = app
        .server
        .post(&format!("/leanfin/labels/{label}/rules/{rule}/delete"))
        .await
        .text();

    for (name, body) in [
        ("create", &created),
        ("list", &listed),
        ("panel", &panel),
        ("delete", &after_delete),
    ] {
        assert!(
            !body.contains("<img src=x"),
            "the {name} response rendered an unescaped rule pattern"
        );
        assert!(
            !body.contains(r#"onerror="window.__xss=1""#),
            "the {name} response rendered an unescaped event handler"
        );
    }
    // The pattern is still shown, just inert.
    assert!(listed.contains("&lt;img src=x"));
}

/// The breakdown payload sits inside a `<script>` body, where escaping quotes is
/// not enough: a category named `</script>` would close the element.
#[tokio::test]
async fn a_label_named_like_a_closing_script_tag_cannot_break_out_of_the_payload() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'seeduser'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let group: i64 =
        sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE name = 'Essentials'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    sqlx::query("INSERT INTO leanfin_labels (user_id, name, group_id) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind("</script><script>window.__xss=1</script>")
        .bind(group)
        .execute(&app.pool)
        .await
        .unwrap();

    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", group.to_string())
        .add_query_param("days", "90")
        .await
        .text();

    assert_eq!(
        body.matches("</script>").count(),
        1,
        "the payload opened a second script element: {body}"
    );
    assert!(
        body.contains("\\u003c/script\\u003e"),
        "expected the markup characters escaped"
    );
}

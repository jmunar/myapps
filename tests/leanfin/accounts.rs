use crate::harness;

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

    sqlx::query("UPDATE accounts SET balance_amount = ?, balance_currency = ? WHERE bank_name = 'Santander'")
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

    sqlx::query("UPDATE accounts SET balance_amount = ?, balance_currency = ? WHERE bank_name = 'Santander'")
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
    sqlx::query("UPDATE accounts SET balance_amount = NULL, balance_currency = NULL")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(!body.contains("account-balance"));
}

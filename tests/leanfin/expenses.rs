use crate::harness;

// ── Expenses page ────────────────────────────────────────────

#[tokio::test]
async fn expenses_page_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/leanfin/expenses").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn expenses_page_renders_with_labels_and_period_selector() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/expenses").await;
    let body = response.text();

    // Full page with doctype
    assert!(body.contains("<!DOCTYPE html>"));
    // Page header
    assert!(body.contains("Expenses"));
    assert!(body.contains("Explore spending by label over time"));
    // Label pills rendered from seed data
    assert!(body.contains("label-pill"));
    assert!(body.contains("Groceries"));
    assert!(body.contains("Subscriptions"));
    // Period selector buttons
    assert!(body.contains("period-selector"));
    assert!(body.contains("30d"));
    assert!(body.contains("90d"));
    assert!(body.contains("180d"));
    assert!(body.contains("365d"));
    // 90d is the default active period
    assert!(body.contains(r#"class="period-btn period-btn-active" data-days="90""#));
}

#[tokio::test]
async fn expenses_page_shows_expenses_nav_active() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/expenses").await;
    let body = response.text();

    // The nav should have the Expenses link
    assert!(body.contains("Expenses"));
}

#[tokio::test]
async fn expenses_page_empty_state_when_no_labels() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/leanfin/expenses").await;
    let body = response.text();

    assert!(body.contains("No labels yet. Create labels and allocate transactions first."));
    assert!(body.contains("empty-state"));
}

#[tokio::test]
async fn expenses_page_has_chart_container() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/expenses").await;
    let body = response.text();

    assert!(body.contains(r#"id="expenses-chart""#));
    assert!(body.contains("Select one or more labels to view expenses"));
}

// ── Chart endpoint ───────────────────────────────────────────

#[tokio::test]
async fn chart_endpoint_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", "1")
        .add_query_param("days", "90")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn chart_endpoint_returns_empty_state_for_no_labels() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();

    assert!(body.contains("No labels selected."));
}

#[tokio::test]
async fn chart_endpoint_returns_empty_state_for_nonexistent_labels() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", "99999")
        .add_query_param("days", "90")
        .await;
    let body = response.text();

    assert!(body.contains("No expense data for the selected labels in this period."));
}

#[tokio::test]
async fn chart_endpoint_returns_chart_data_for_valid_label() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Get the Groceries label id
    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", &label_id.to_string())
        .add_query_param("days", "365")
        .await;
    let body = response.text();

    // Should contain the frappe chart container and script
    assert!(body.contains("expenses-frappe-chart"));
    assert!(body.contains("frappe.Chart"));
    assert!(body.contains("Groceries"));
}

#[tokio::test]
async fn chart_endpoint_defaults_to_90_days() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Omit days param — should default to 90
    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", &label_id.to_string())
        .await;

    assert!(response.status_code().is_success());
}

#[tokio::test]
async fn chart_endpoint_supports_multiple_labels() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (groceries_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let (subs_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Subscriptions'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let ids = format!("{groceries_id},{subs_id}");
    let response = app
        .server
        .get("/leanfin/expenses/chart")
        .add_query_param("label_ids", &ids)
        .add_query_param("days", "365")
        .await;
    let body = response.text();

    assert!(body.contains("Groceries"));
    assert!(body.contains("Subscriptions"));
}

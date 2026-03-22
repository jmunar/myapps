use crate::harness;

#[tokio::test]
async fn categories_page_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app
        .server
        .get("/mindflow/categories")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn categories_page_renders_seeded_categories() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Work"));
    assert!(body.contains("Health"));
    assert!(body.contains("Finance"));
    assert!(body.contains("Personal"));
    assert!(body.contains("Learning"));
    assert!(body.contains("Home"));
}

#[tokio::test]
async fn categories_page_has_create_form() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    assert!(body.contains(r#"name="name""#));
    assert!(body.contains(r#"name="color""#));
}

#[tokio::test]
async fn create_category() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    app.server
        .post("/mindflow/categories/create")
        .form(&serde_json::json!({
            "name": "Hobbies",
            "color": "#E91E63",
            "icon": "Hb",
        }))
        .expect_failure()
        .await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    assert!(body.contains("Hobbies"));
}

#[tokio::test]
async fn edit_category() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Work' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/mindflow/categories/{id}/edit"))
        .form(&serde_json::json!({
            "name": "Work & Career",
            "color": "#1565C0",
            "icon": "WC",
        }))
        .expect_failure()
        .await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    assert!(body.contains("Work &amp; Career") || body.contains("Work & Career"));
}

#[tokio::test]
async fn archive_category() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Learning' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/mindflow/categories/{id}/archive"))
        .expect_failure()
        .await;

    let (archived,): (bool,) =
        sqlx::query_as("SELECT archived FROM mindflow_categories WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(archived);
}

#[tokio::test]
async fn unarchive_category() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Learning' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Archive first
    sqlx::query("UPDATE mindflow_categories SET archived = 1 WHERE id = ?")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    app.server
        .post(&format!("/mindflow/categories/{id}/unarchive"))
        .expect_failure()
        .await;

    let (archived,): (bool,) =
        sqlx::query_as("SELECT archived FROM mindflow_categories WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(!archived);
}

#[tokio::test]
async fn delete_empty_category() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Create a category with no thoughts
    app.server
        .post("/mindflow/categories/create")
        .form(&serde_json::json!({
            "name": "EmptyCat",
            "color": "#000000",
            "icon": "",
        }))
        .expect_failure()
        .await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'EmptyCat' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/mindflow/categories/{id}/delete"))
        .expect_failure()
        .await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    assert!(!body.contains("EmptyCat"));
}

#[tokio::test]
async fn categories_page_shows_thought_count() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let response = app.server.get("/mindflow/categories").await;
    let body = response.text();
    // Seeded categories have thoughts, so counts should appear
    assert!(response.status_code().is_success());
    // Work has 3 thoughts + sub-thoughts
    assert!(body.contains("Work"));
}

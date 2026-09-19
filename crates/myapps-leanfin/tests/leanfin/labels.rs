#[tokio::test]
async fn labels_page_renders_seeded_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("Groceries"));
    assert!(body.contains("Subscriptions"));
    assert!(body.contains("Entertainment"));
}

#[tokio::test]
async fn create_label_appears_in_list() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("test", "pass").await;

    // Create a label (POST redirects with 303)
    app.server
        .post("/leanfin/labels/create")
        .form(&serde_json::json!({"name": "TestLabel"}))
        .expect_failure()
        .await;

    // Verify it shows up
    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("TestLabel"));
}

#[tokio::test]
async fn delete_label_removes_from_list() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Entertainment'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label_id}/delete"))
        .expect_failure()
        .await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(!body.contains("Entertainment"));
}

#[tokio::test]
async fn edit_label_updates_name() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label_id}/edit"))
        .form(&serde_json::json!({"name": "Food & Groceries"}))
        .expect_failure()
        .await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("Food &amp; Groceries") || body.contains("Food & Groceries"));
}

#[tokio::test]
async fn label_rules_panel_loads() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/labels/{label_id}/rules"))
        .await;
    let body = response.text();
    // Seeded rules for Groceries: counterparty=Mercadona, counterparty=Carrefour
    assert!(body.contains("Mercadona"));
    assert!(body.contains("Carrefour"));
}

#[tokio::test]
async fn create_rule_for_label() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/labels/{label_id}/rules/create"))
        .form(&serde_json::json!({
            "field": "counterparty",
            "pattern": "Lidl",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Lidl"));
}

// ── Label groups ─────────────────────────────────────────────

use myapps_leanfin::LeanFinApp;

async fn group_id(pool: &sqlx::SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("group {name} not found: {e}"))
}

#[tokio::test]
async fn labels_page_groups_labels_under_their_group() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app.server.get("/leanfin/labels").await.text();

    assert!(body.contains("lf-group"));
    assert!(body.contains("Essentials"));
    assert!(body.contains("Lifestyle"));
    // Labels are a plain enumeration of chips; their controls live in the panel
    // that opens below, not on the row.
    assert!(body.contains("lf-chip"));
    assert!(body.contains("lf-detail"));
    assert!(!body.contains("label-group-select"));
    // Colour is no longer a label attribute.
    assert!(!body.contains(r#"<input type="color" name="color""#));
    // Create label comes before create group, which comes before the groups list.
    let create_label = body.find("/leanfin/labels/create").unwrap();
    let create_group = body.find("/leanfin/label-groups/create").unwrap();
    let groups_list = body.find("lf-group-head").unwrap();
    assert!(create_label < create_group, "create label must come first");
    assert!(
        create_group < groups_list,
        "create group must precede the groups list"
    );
}

#[tokio::test]
async fn a_new_user_gets_an_other_group_and_labels_land_in_it() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.login_as("test", "pass").await;

    app.server
        .post("/leanfin/labels/create")
        .form(&serde_json::json!({"name": "Coffee"}))
        .expect_failure()
        .await;

    let (group_name,): (String,) = sqlx::query_as(
        r#"SELECT g.name FROM leanfin_labels l
           JOIN leanfin_label_groups g ON l.group_id = g.id
           WHERE l.name = 'Coffee'"#,
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(group_name, "No group");
}

#[tokio::test]
async fn create_group_then_move_a_label_into_it() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    app.server
        .post("/leanfin/label-groups/create")
        .form(&serde_json::json!({"name": "Travel"}))
        .expect_failure()
        .await;

    let travel = group_id(&app.pool, "Travel").await;
    let (label,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Transport'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label}/group"))
        .form(&serde_json::json!({"group_id": travel}))
        .expect_failure()
        .await;

    let (moved,): (i64,) = sqlx::query_as("SELECT group_id FROM leanfin_labels WHERE id = ?")
        .bind(label)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(moved, travel);
}

#[tokio::test]
async fn deleting_a_group_returns_its_labels_to_other() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let lifestyle = group_id(&app.pool, "Lifestyle").await;
    let other = group_id(&app.pool, "No group").await;

    app.server
        .post(&format!("/leanfin/label-groups/{lifestyle}/delete"))
        .expect_failure()
        .await;

    let (orphans,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM leanfin_labels WHERE group_id = ?")
            .bind(lifestyle)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(orphans, 0);

    let (dining_group,): (i64,) =
        sqlx::query_as("SELECT group_id FROM leanfin_labels WHERE name = 'Dining'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(dining_group, other, "labels fall back to Other");
}

#[tokio::test]
async fn the_default_group_cannot_be_deleted() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let other = group_id(&app.pool, "No group").await;
    app.server
        .post(&format!("/leanfin/label-groups/{other}/delete"))
        .expect_failure()
        .await;

    let (still_there,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_groups WHERE id = ?")
            .bind(other)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(still_there, 1);
}

#[tokio::test]
async fn a_label_cannot_be_moved_into_another_users_group() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let (label,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Dining'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT group_id FROM leanfin_labels WHERE id = ?")
        .bind(label)
        .fetch_one(&app.pool)
        .await
        .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label}/group"))
        .form(&serde_json::json!({"group_id": 99999}))
        .expect_failure()
        .await;

    let after: i64 = sqlx::query_scalar("SELECT group_id FROM leanfin_labels WHERE id = ?")
        .bind(label)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_ne!(after, 99999);
    let other = group_id(&app.pool, "No group").await;
    assert!(after == before || after == other);
}

#[tokio::test]
async fn badge_colour_comes_from_the_group_not_the_label() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let essentials = group_id(&app.pool, "Essentials").await;
    let expected = myapps_leanfin::colors::group_color(essentials);

    let body = app.server.get("/leanfin/labels").await.text();
    assert!(
        body.contains(&format!("--label-color:{expected}")),
        "expected the Essentials group colour {expected} on its badges"
    );
}

// ── Detail panels ────────────────────────────────────────────

#[tokio::test]
async fn group_panel_offers_rename_and_delete() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let lifestyle = group_id(&app.pool, "Lifestyle").await;
    let body = app
        .server
        .get(&format!("/leanfin/label-groups/{lifestyle}/panel"))
        .await
        .text();

    assert!(body.contains("lf-panel"));
    assert!(body.contains(&format!("/leanfin/label-groups/{lifestyle}/edit")));
    assert!(body.contains(&format!("/leanfin/label-groups/{lifestyle}/delete")));
    assert!(body.contains(r#"value="Lifestyle""#));
}

#[tokio::test]
async fn the_default_groups_panel_offers_no_delete() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let other = group_id(&app.pool, "No group").await;
    let body = app
        .server
        .get(&format!("/leanfin/label-groups/{other}/panel"))
        .await
        .text();

    // Renaming it is fine; deleting it is not offered at all.
    assert!(body.contains(&format!("/leanfin/label-groups/{other}/edit")));
    assert!(!body.contains(&format!("/leanfin/label-groups/{other}/delete")));
    assert!(body.contains("cannot be deleted"));
}

#[tokio::test]
async fn label_panel_carries_rules_group_and_delete() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let (label,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let body = app
        .server
        .get(&format!("/leanfin/labels/{label}/panel"))
        .await
        .text();

    assert!(body.contains("lf-panel"));
    assert!(body.contains(&format!("/leanfin/labels/{label}/edit")));
    assert!(body.contains(&format!("/leanfin/labels/{label}/group")));
    assert!(body.contains(&format!("/leanfin/labels/{label}/delete")));
    // Rules are edited inside the panel.
    assert!(body.contains("Mercadona"));
    assert!(body.contains("rules-panel"));
}

#[tokio::test]
async fn panels_for_another_users_rows_render_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    assert!(
        app.server
            .get("/leanfin/labels/999999/panel")
            .await
            .text()
            .is_empty()
    );
    assert!(
        app.server
            .get("/leanfin/label-groups/999999/panel")
            .await
            .text()
            .is_empty()
    );
}

#[tokio::test]
async fn a_group_can_be_renamed() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let lifestyle = group_id(&app.pool, "Lifestyle").await;
    app.server
        .post(&format!("/leanfin/label-groups/{lifestyle}/edit"))
        .form(&serde_json::json!({"name": "Fun"}))
        .expect_failure()
        .await;

    let (name,): (String,) = sqlx::query_as("SELECT name FROM leanfin_label_groups WHERE id = ?")
        .bind(lifestyle)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name, "Fun");
}

#[tokio::test]
async fn renaming_the_default_group_keeps_it_default() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let other = group_id(&app.pool, "No group").await;
    app.server
        .post(&format!("/leanfin/label-groups/{other}/edit"))
        .form(&serde_json::json!({"name": "Unsorted"}))
        .expect_failure()
        .await;

    // `is_default`, not the name, is what protects it — so it must survive a
    // rename and still refuse deletion.
    app.server
        .post(&format!("/leanfin/label-groups/{other}/delete"))
        .expect_failure()
        .await;

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_groups WHERE id = ? AND is_default = 1")
            .bind(other)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "the renamed default group must still exist");

    // And a new label still lands in it rather than spawning a second default.
    app.server
        .post("/leanfin/labels/create")
        .form(&serde_json::json!({"name": "Sundries"}))
        .expect_failure()
        .await;

    let (defaults,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_groups WHERE is_default = 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(defaults, 1, "exactly one default group per user");
}

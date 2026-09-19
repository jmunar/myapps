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

// ── Group form error paths ───────────────────────────────────
//
// Every one of these handlers redirects to /leanfin/labels whatever happens, so
// the response says nothing — the assertions have to look at the table.

#[tokio::test]
async fn creating_a_group_with_a_blank_name_creates_nothing() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_groups")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    for name in ["", "   "] {
        let response = app
            .server
            .post("/leanfin/label-groups/create")
            .form(&serde_json::json!({ "name": name }))
            .expect_failure()
            .await;
        assert_eq!(response.status_code(), 303);
    }

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_groups")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(after, before, "a blank name created a group");
}

#[tokio::test]
async fn creating_a_group_that_already_exists_does_not_duplicate_it() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    app.server
        .post("/leanfin/label-groups/create")
        .form(&serde_json::json!({"name": "Essentials"}))
        .expect_failure()
        .await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_label_groups WHERE name = 'Essentials'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn renaming_a_group_to_a_blank_name_is_ignored() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let lifestyle = group_id(&app.pool, "Lifestyle").await;
    app.server
        .post(&format!("/leanfin/label-groups/{lifestyle}/edit"))
        .form(&serde_json::json!({"name": "  "}))
        .expect_failure()
        .await;

    let name: String = sqlx::query_scalar("SELECT name FROM leanfin_label_groups WHERE id = ?")
        .bind(lifestyle)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name, "Lifestyle");
}

#[tokio::test]
async fn renaming_a_group_onto_an_existing_name_leaves_both_alone() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let lifestyle = group_id(&app.pool, "Lifestyle").await;

    // (user_id, name) is unique — the clash must not take the page down.
    let response = app
        .server
        .post(&format!("/leanfin/label-groups/{lifestyle}/edit"))
        .form(&serde_json::json!({"name": "Essentials"}))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let name: String = sqlx::query_scalar("SELECT name FROM leanfin_label_groups WHERE id = ?")
        .bind(lifestyle)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name, "Lifestyle", "the rename should have been refused");

    let body = app.server.get("/leanfin/labels").await.text();
    assert!(body.contains("Essentials") && body.contains("Lifestyle"));
}

#[tokio::test]
async fn an_empty_group_says_so_and_counts_its_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app.server.get("/leanfin/labels").await.text();

    // The seed leaves the default group empty and puts four labels in
    // Essentials; both facts are on the page.
    assert!(body.contains("No labels in this group yet."));
    assert!(body.contains("lf-group-empty"));

    let essentials = group_id(&app.pool, "Essentials").await;
    let expected: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_labels WHERE group_id = ?")
            .bind(essentials)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(expected > 0, "the fixture should put labels in Essentials");
    assert!(
        body.contains(&format!(
            r#"<span class="lf-count text-secondary text-sm">{expected}</span>"#
        )),
        "each group heading should carry its label count"
    );
}

// ── Logged-out redirects for the new routes ──────────────────

#[tokio::test]
async fn every_label_group_route_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;

    for route in ["/leanfin/labels/1/panel", "/leanfin/label-groups/1/panel"] {
        let response = app.server.get(route).expect_failure().await;
        assert_eq!(response.status_code(), 303, "{route} did not redirect");
    }

    for route in [
        "/leanfin/label-groups/create",
        "/leanfin/label-groups/1/edit",
        "/leanfin/label-groups/1/delete",
        "/leanfin/labels/1/group",
    ] {
        let response = app.server.post(route).expect_failure().await;
        assert_eq!(response.status_code(), 303, "{route} did not redirect");
    }
}

#[tokio::test]
async fn a_label_whose_group_went_missing_is_repaired_on_render() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    // `ON DELETE SET NULL` is the backstop behind group deletion, so a NULL
    // group_id is reachable in a deployed database. It must not make the label
    // disappear from the page.
    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let orphan: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_labels (user_id, name, group_id) VALUES (?, 'Orphan', NULL) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let body = app.server.get("/leanfin/labels").await.text();
    assert!(body.contains("Orphan"), "an ungrouped label vanished");

    let repaired: Option<i64> =
        sqlx::query_scalar("SELECT group_id FROM leanfin_labels WHERE id = ?")
            .bind(orphan)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let default: i64 =
        sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE is_default = 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        repaired,
        Some(default),
        "it should be moved to the default group"
    );
}

#[tokio::test]
async fn breakdown_pills_use_the_group_colour() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let essentials = group_id(&app.pool, "Essentials").await;
    let lifestyle = group_id(&app.pool, "Lifestyle").await;

    let body = app.server.get("/leanfin/breakdown").await.text();

    for g in [essentials, lifestyle] {
        let colour = myapps_leanfin::colors::group_color(g);
        let at = body
            .find(&format!(r#"data-group-id="{g}""#))
            .unwrap_or_else(|| panic!("no pill for group {g}"));
        assert!(
            body[at.saturating_sub(120)..at].contains(&format!("--label-color:{colour}")),
            "group {g} should be painted {colour}"
        );
    }
}

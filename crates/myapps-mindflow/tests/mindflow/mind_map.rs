#[tokio::test]
async fn mind_map_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    let response = app.server.get("/mindflow").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn mind_map_page_renders_with_capture_form() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains(r#"hx-post="/mindflow/capture""#));
    assert!(body.contains("Mind Map"));
}

#[tokio::test]
async fn mind_map_page_shows_category_dropdown() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("Work"));
    assert!(body.contains("Health"));
    assert!(body.contains("Finance"));
}

#[tokio::test]
async fn mind_map_page_has_navigation() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("/mindflow/inbox"));
    assert!(body.contains("/mindflow/actions"));
    assert!(body.contains("/mindflow/categories"));
}

#[tokio::test]
async fn map_data_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    let response = app.server.get("/mindflow/map-data").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn map_data_endpoint_returns_json_with_nodes_and_links() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow/map-data").await;
    let body = response.text();
    assert!(body.contains("nodes"));
    assert!(body.contains("links"));
    // Should contain seeded category names
    assert!(body.contains("Work"));
    assert!(body.contains("Health"));
}

#[tokio::test]
async fn capture_thought_returns_feedback() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app
        .server
        .post("/mindflow/capture")
        .form(&serde_json::json!({
            "content": "A brand new thought",
            "category_id": "",
            "parent_thought_id": "",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Captured!"));
}

#[tokio::test]
async fn capture_thought_with_category() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let (cat_id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Work' AND user_id = (SELECT id FROM users WHERE username = 'seeduser')")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post("/mindflow/capture")
        .form(&serde_json::json!({
            "content": "Work thought",
            "category_id": cat_id.to_string(),
            "parent_thought_id": "",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Captured!"));

    // Verify it was stored in the correct category
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mindflow_thoughts WHERE content = 'Work thought' AND category_id = ?",
    )
    .bind(cat_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn mind_map_page_shows_inbox_badge() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    // Seed data has 3 uncategorized (inbox) thoughts
    assert!(body.contains("Inbox"));
}

// mind-map.js reads its empty-state label off #mindmap and the base path off
// <html data-base>. Nothing in CI parses that file, so pin the attributes.
#[tokio::test]
async fn map_page_gives_the_script_its_container_and_label() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let body = app.server.get("/mindflow").await.text();
    assert!(body.contains(r#"id="mindmap""#));
    assert!(
        body.contains("data-empty-label="),
        "mind-map.js reads its empty-state text from #mindmap"
    );
    assert!(
        body.contains("/static/d3.v7.min.js?v="),
        "d3 must be cache-busted"
    );
    // refreshMap() is called from an hx-on attribute, so it has to stay global.
    assert!(body.contains("window.refreshMap"));
}

// The map is drawn entirely from `/mindflow/map-data`, which the script now
// consumes from another file. `body.contains("nodes")` would pass on an empty
// graph, so check the fields d3 actually binds: the node ids the links join
// on, the `type` that picks the circle size, the colour and the thought id the
// click handler navigates to.
#[tokio::test]
async fn map_data_carries_the_fields_the_script_binds() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let data: serde_json::Value = app.server.get("/mindflow/map-data").await.json();
    let nodes = data["nodes"].as_array().expect("nodes must be an array");
    let links = data["links"].as_array().expect("links must be an array");
    assert!(!nodes.is_empty(), "the seeded map rendered no nodes");
    assert!(!links.is_empty(), "the seeded map rendered no links");

    let mut ids = std::collections::HashSet::new();
    let mut categories = 0;
    let mut thoughts = 0;
    for n in nodes {
        let id = n["id"].as_str().expect("every node needs a string id");
        assert!(ids.insert(id), "duplicate node id {id}");
        assert!(!n["name"].as_str().unwrap_or("").is_empty());
        match n["type"].as_str().expect("every node needs a type") {
            "category" => {
                categories += 1;
                // The circle is filled from it; a null falls back to grey.
                assert!(n["color"].is_string(), "category {id} has no colour");
                assert!(n["thought_id"].is_null());
            }
            "thought" => {
                thoughts += 1;
                assert!(
                    n["thought_id"].is_i64(),
                    "clicking thought {id} must navigate somewhere"
                );
            }
            other => panic!("unknown node type {other}"),
        }
    }
    assert!(categories > 0 && thoughts > 0);

    // d3.forceLink resolves both ends by id: an endpoint with no node throws.
    for l in links {
        for end in ["source", "target"] {
            let v = l[end].as_str().expect("link ends are node ids");
            assert!(ids.contains(v), "link {end} {v} has no node");
        }
    }
}

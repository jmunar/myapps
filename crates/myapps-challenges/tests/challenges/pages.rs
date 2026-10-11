use crate::{app, current, insert_problem, login, mark};
use myapps_challenges::ChallengesApp;

#[tokio::test]
async fn picker_requires_authentication() {
    let app = app().await;
    let response = app.server.get("/challenges").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn picker_says_how_to_import_an_empty_dataset() {
    let app = app().await;
    login(&app).await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("UGPhysics"));
    assert!(body.contains("Hendrycks MATH"));
    assert!(body.contains("myapps-challenges-prep ugphysics"));
    assert!(body.contains("myapps import --app challenges --dataset ugphysics.sqlite"));
    assert!(body.contains("myapps-challenges-prep hendrycks-math"));
    assert!(body.contains("myapps import --app challenges --dataset hendrycks-math.sqlite"));
    assert!(!body.contains(r#"href="/challenges/practice/"#));
}

#[tokio::test]
async fn seeded_picker_offers_both_datasets() {
    let app = app().await;
    app.seed_and_login(&ChallengesApp).await;
    let body = app.server.get("/challenges").await.text();
    assert!(!body.contains("myapps import"));
    assert!(body.contains(r#"href="/challenges/practice/ugphysics""#));
    assert!(body.contains(r#"href="/challenges/practice/hendrycks-math""#));
    assert!(body.contains("6 problems · 3 subjects"));
    // The seed's history: 2 of 3 right.
    assert!(body.contains("67%"));
}

#[tokio::test]
async fn picker_says_continue_once_a_problem_is_in_progress() {
    let app = app().await;
    login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    let start =
        r#"href="/challenges/practice/ugphysics" class="btn btn-primary challenges-start">Start<"#;
    let resume = r#"href="/challenges/practice/ugphysics" class="btn btn-primary challenges-start">Continue<"#;

    let picker = app.server.get("/challenges").await;
    assert_eq!(picker.header("cache-control"), "no-store");
    assert!(picker.text().contains(start));
    app.server.get("/challenges/practice/ugphysics").await;
    assert!(app.server.get("/challenges").await.text().contains(resume));
}

// ── Hidden datasets ─────────────────────────────────────────

#[tokio::test]
async fn hiding_a_dataset_moves_it_to_the_hidden_list() {
    let app = app().await;
    login(&app).await;

    let fragment = app
        .server
        .post("/challenges/datasets/ugphysics/hidden")
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "hidden": "1" }))
        .await
        .text();
    assert!(fragment.starts_with(r#"<div id="challenges-picker">"#));
    assert!(fragment.contains("Hidden datasets"));
    // The card is gone; what is left for it is the form to show it again.
    assert!(!fragment.contains("--dataset ugphysics"));
    assert!(fragment.contains("--dataset hendrycks-math"));
    assert!(fragment.contains(r#"<input type="hidden" name="hidden" value="0">"#));

    // It sticks across page loads.
    let body = app.server.get("/challenges").await.text();
    assert!(!body.contains("--dataset ugphysics"));
    assert!(body.contains("Hidden datasets"));

    // And showing it again brings the card back.
    let response = app
        .server
        .post("/challenges/datasets/ugphysics/hidden")
        .form(&serde_json::json!({ "hidden": "0" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("--dataset ugphysics"));
    assert!(!body.contains("Hidden datasets"));
}

#[tokio::test]
async fn hiding_every_dataset_says_so() {
    let app = app().await;
    login(&app).await;
    for key in ["ugphysics", "hendrycks-math"] {
        app.server
            .post(&format!("/challenges/datasets/{key}/hidden"))
            .form(&serde_json::json!({ "hidden": "1" }))
            .expect_failure()
            .await;
    }
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("Every dataset is hidden"));
    assert!(!body.contains("challenges-datasets"));
}

#[tokio::test]
async fn hiding_is_per_user() {
    let app = app().await;
    login(&app).await;
    app.server
        .post("/challenges/datasets/ugphysics/hidden")
        .form(&serde_json::json!({ "hidden": "1" }))
        .expect_failure()
        .await;

    app.server.get("/logout").expect_failure().await;
    app.login_as("other", "pass").await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("--dataset ugphysics"));
    assert!(!body.contains("Hidden datasets"));
}

#[tokio::test]
async fn hiding_an_unknown_dataset_changes_nothing() {
    let app = app().await;
    login(&app).await;

    let fragment = app
        .server
        .post("/challenges/datasets/u-math/hidden")
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "hidden": "1" }))
        .await
        .text();
    assert!(fragment.starts_with(r#"<div id="challenges-picker">"#));
    assert!(fragment.contains("--dataset ugphysics"));
    assert!(fragment.contains("--dataset hendrycks-math"));
    assert!(!fragment.contains("Hidden datasets"));

    let response = app
        .server
        .post("/challenges/datasets/u-math/hidden")
        .form(&serde_json::json!({ "hidden": "1" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_hidden_datasets")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn hiding_requires_authentication() {
    let app = app().await;
    let response = app
        .server
        .post("/challenges/datasets/ugphysics/hidden")
        .form(&serde_json::json!({ "hidden": "1" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_ne!(response.header("location"), "/challenges");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_hidden_datasets")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn a_hidden_dataset_can_still_be_practised() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "ugphysics", "Optics", 1, "still here").await;
    app.server
        .post("/challenges/datasets/ugphysics/hidden")
        .form(&serde_json::json!({ "hidden": "1" }))
        .expect_failure()
        .await;

    let body = app
        .server
        .get("/challenges/practice/ugphysics")
        .await
        .text();
    assert!(body.contains("still here"));
    assert!(body.contains(&format!(r#"name="problem" value="{id}""#)));
}

#[tokio::test]
async fn picker_labels_render_in_spanish() {
    let app = app().await;
    login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    app.server
        .post("/settings/language")
        .form(&serde_json::json!({ "language": "es", "redirect": "/challenges" }))
        .expect_failure()
        .await;

    app.server.get("/challenges/practice/ugphysics").await;
    let fragment = app
        .server
        .post("/challenges/datasets/hendrycks-math/hidden")
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "hidden": "1" }))
        .await
        .text();
    assert!(fragment.contains(">Continuar<"));
    assert!(fragment.contains(">Ocultar<"));
    assert!(fragment.contains("Conjuntos ocultos"));
    assert!(fragment.contains(">Mostrar<"));
    assert!(!fragment.contains("Hidden datasets"));

    app.server
        .post("/challenges/datasets/ugphysics/hidden")
        .form(&serde_json::json!({ "hidden": "1" }))
        .expect_failure()
        .await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("Todos los conjuntos están ocultos"));
}

// ── Practice ────────────────────────────────────────────────

#[tokio::test]
async fn practice_page_renders_problem_solution_and_marking_form() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "ugphysics", "Relativity", 2, "Find $\\gamma$.").await;

    let response = app.server.get("/challenges/practice/ugphysics").await;
    assert_eq!(response.header("cache-control"), "no-store");
    let body = response.text();
    assert!(body.contains("Relativity"));
    assert!(body.contains("Find $\\gamma$."));
    assert!(body.contains("the solution"));
    assert!(body.contains("Level 2 of 3 (Laws Application)"));
    assert!(body.contains(r#"hx-post="/challenges/practice/ugphysics/attempt""#));
    assert!(body.contains(r#"hx-post="/challenges/practice/ugphysics/skip""#));
    assert!(body.contains(r##"hx-target="#challenges-practice""##));
    assert!(body.contains(r#"<div id="challenges-practice">"#));
    assert!(body.contains(&format!(r#"name="problem" value="{id}""#)));
    assert!(body.contains(r#"name="correct" value="1""#));
    assert!(body.contains(r#"name="correct" value="0""#));
    assert!(body.contains("<details class=\"card challenges-reveal\">"));
}

/// `challenges-math.js` finds its elements by this attribute, and retypesets
/// after the htmx swap that brings in the next problem. Nothing type-checks
/// either pairing: a rename on one side breaks only in the browser.
#[tokio::test]
async fn practice_page_renders_what_the_math_script_looks_for() {
    let script = include_str!("../../static/challenges-math.js");
    assert!(script.contains("[data-challenges-math]"));
    assert!(script.contains("renderMathInElement"));
    assert!(script.contains("htmx:afterSwap"));

    let app = app().await;
    login(&app).await;
    insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "1+1").await;
    let body = app
        .server
        .get("/challenges/practice/hendrycks-math")
        .await
        .text();
    assert!(body.contains("data-challenges-math>"));
    assert!(body.contains("/static/katex/katex.min.js"));
    assert!(body.contains("/static/katex/auto-render.min.js"));
    assert!(body.contains("/static/katex/katex.min.css"));
}

#[tokio::test]
async fn problem_text_is_escaped() {
    let app = app().await;
    login(&app).await;
    insert_problem(
        &app.pool,
        "ugphysics",
        "<b>Optics</b>",
        1,
        "</script><img src=x onerror=alert(1)>",
    )
    .await;
    let body = app
        .server
        .get("/challenges/practice/ugphysics")
        .await
        .text();
    assert!(!body.contains("<img src=x"));
    assert!(body.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!body.contains("<b>Optics</b>"));
    assert!(body.contains("<h1>&lt;b&gt;Optics&lt;/b&gt;</h1>"));
}

#[tokio::test]
async fn the_problem_stays_until_it_is_marked_or_skipped() {
    let app = app().await;
    let user_id = login(&app).await;
    for i in 0..10 {
        insert_problem(&app.pool, "ugphysics", "Optics", 1, &format!("p{i}")).await;
    }

    app.server.get("/challenges/practice/ugphysics").await;
    let first = current(&app.pool, user_id, "ugphysics").await.unwrap();
    for _ in 0..5 {
        let body = app
            .server
            .get("/challenges/practice/ugphysics")
            .await
            .text();
        assert!(body.contains(&format!(r#"name="problem" value="{first}""#)));
    }

    // Skipping moves on, and the new one sticks in turn.
    let fragment = app
        .server
        .post("/challenges/practice/ugphysics/skip")
        .add_header("hx-request", "true")
        .await
        .text();
    let second = current(&app.pool, user_id, "ugphysics").await.unwrap();
    assert_ne!(second, first);
    assert!(fragment.contains(&format!(r#"name="problem" value="{second}""#)));
    assert!(!fragment.contains("<html"));

    // Nothing was recorded by looking or skipping.
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
}

#[tokio::test]
async fn each_dataset_keeps_its_own_problem() {
    let app = app().await;
    let user_id = login(&app).await;
    let physics = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    let maths = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "m").await;
    app.server.get("/challenges/practice/ugphysics").await;
    app.server.get("/challenges/practice/hendrycks-math").await;
    assert_eq!(
        current(&app.pool, user_id, "ugphysics").await,
        Some(physics)
    );
    assert_eq!(
        current(&app.pool, user_id, "hendrycks-math").await,
        Some(maths)
    );
}

#[tokio::test]
async fn marking_swaps_in_the_next_problem() {
    let app = app().await;
    let user_id = login(&app).await;
    let first = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    let second = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p2").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();
    let other = if shown == first { second } else { first };

    // Wrong at the bottom: no level change, so no notice, just the next one.
    let fragment = mark(&app, "ugphysics", shown, false).await.text();
    assert!(fragment.contains(&format!(r#"name="problem" value="{other}""#)));
    assert!(!fragment.contains("challenges-level-change"));
    assert_eq!(current(&app.pool, user_id, "ugphysics").await, Some(other));
}

#[tokio::test]
async fn a_plain_form_post_redirects_back_to_the_practice_page() {
    let app = app().await;
    let user_id = login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p2").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();

    let response = app
        .server
        .post("/challenges/practice/ugphysics/attempt")
        .form(&serde_json::json!({ "problem": shown, "correct": "0" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(
        response.header("location"),
        "/challenges/practice/ugphysics"
    );
    assert_ne!(current(&app.pool, user_id, "ugphysics").await, Some(shown));
}

#[tokio::test]
async fn a_plain_skip_redirects_back_to_the_practice_page() {
    let app = app().await;
    let user_id = login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p2").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();

    let response = app
        .server
        .post("/challenges/practice/ugphysics/skip")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(
        response.header("location"),
        "/challenges/practice/ugphysics"
    );
    assert_ne!(current(&app.pool, user_id, "ugphysics").await, Some(shown));
}

#[tokio::test]
async fn skipping_an_empty_dataset_tells_htmx_to_navigate() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .post("/challenges/practice/ugphysics/skip")
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        response.header("hx-redirect"),
        "/challenges/practice/ugphysics"
    );
    assert!(response.text().is_empty());
}

#[tokio::test]
async fn a_wrong_answer_that_drops_a_level_says_so_above_the_next_problem() {
    let app = app().await;
    let user_id = login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Optics", 2, "p2").await;
    sqlx::query(
        "INSERT INTO challenges_progress (user_id, dataset, subject, level, streak, fast_start)
         VALUES (?, 'ugphysics', 'Optics', 2, 0, 0)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();
    app.server.get("/challenges/practice/ugphysics").await;
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();

    let fragment = mark(&app, "ugphysics", shown, false).await.text();
    assert!(fragment.contains("Level down"));
    assert!(fragment.contains("challenges-level-change challenges-down"));
    assert!(fragment.contains("Optics: Level 2 → 1 of 3"));
    assert!(!fragment.contains("Level up"));
    assert!(fragment.find("Level down") < fragment.find("challenges-problem"));
}

#[tokio::test]
async fn first_right_answer_levels_up_and_says_so_above_the_next_problem() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    let b = insert_problem(&app.pool, "ugphysics", "Relativity", 2, "p2").await;
    app.server.get("/challenges/practice/ugphysics").await;
    // The cold start draws from levels 1 and 2, so either can come first.
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();
    let other = if shown == a { b } else { a };

    let fragment = mark(&app, "ugphysics", shown, true).await.text();
    assert!(fragment.contains("Level up"));
    assert!(fragment.contains("Relativity: Level 1 → 2 of 3"));
    assert!(fragment.contains(&format!(r#"name="problem" value="{other}""#)));
    // The notice comes before the problem it introduces.
    assert!(fragment.find("Level up") < fragment.find("challenges-problem"));
}

#[tokio::test]
async fn marking_a_problem_that_is_no_longer_current_records_nothing() {
    let app = app().await;
    let user_id = login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 2, "p2").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let id = current(&app.pool, user_id, "ugphysics").await.unwrap();

    // A double tap: the first moves on, the second is for a stale problem.
    let first = mark(&app, "ugphysics", id, true).await;
    assert!(first.text().contains("Level up"));
    let next = current(&app.pool, user_id, "ugphysics").await.unwrap();
    let second = mark(&app, "ugphysics", id, true).await.text();
    assert!(!second.contains("Level up"));
    assert!(second.contains(&format!(r#"name="problem" value="{next}""#)));

    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
    let level: i64 = sqlx::query_scalar("SELECT level FROM challenges_progress")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(level, 2);
    assert_eq!(current(&app.pool, user_id, "ugphysics").await, Some(next));
}

#[tokio::test]
async fn old_problem_links_land_on_the_problem_in_progress() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Optics", 1, "a").await;
    let b = insert_problem(&app.pool, "ugphysics", "Optics", 1, "b").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let shown = current(&app.pool, user_id, "ugphysics").await.unwrap();
    let other = if shown == a { b } else { a };

    let response = app
        .server
        .get(&format!("/challenges/problems/{other}"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(
        response.header("location"),
        "/challenges/practice/ugphysics"
    );
    assert_eq!(current(&app.pool, user_id, "ugphysics").await, Some(shown));
}

#[tokio::test]
async fn unknown_problem_is_404() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .get("/challenges/problems/999")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
    assert!(response.text().contains("Problem not found."));
}

#[tokio::test]
async fn an_unknown_dataset_goes_back_to_the_picker() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .get("/challenges/practice/u-math")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges");

    // htmx is told to navigate rather than handed a redirect to follow.
    let response = app
        .server
        .post("/challenges/practice/u-math/skip")
        .add_header("hx-request", "true")
        .await;
    assert_eq!(response.header("hx-redirect"), "/challenges");
}

use crate::{app, insert_problem, login};
use myapps_challenges::ops::{self, MistakeCounts};
use myapps_challenges::{ChallengesApp, dataset::Dataset};
use myapps_test_harness::TestApp;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Move every attempt on `problem` `days` into the past, so it has waited
/// that long (and is out of the double-submit window).
async fn age(pool: &SqlitePool, problem: i64, days: i64) {
    sqlx::query(
        "UPDATE challenges_attempts SET created_at = datetime(created_at, ?) WHERE problem_id = ?",
    )
    .bind(format!("-{days} days"))
    .bind(problem)
    .execute(pool)
    .await
    .unwrap();
}

/// Get `problem` wrong in practice and let 3 days (the default cooling-off)
/// pass, so it is due.
async fn due_mistake(app: &TestApp, user_id: i64, problem: i64) {
    ops::record_attempt(&app.pool, user_id, problem, false)
        .await
        .unwrap();
    age(&app.pool, problem, 3).await;
}

/// Mark the mistake being replayed the way the htmx form does.
async fn replay(
    app: &TestApp,
    dataset: &str,
    problem: i64,
    correct: bool,
) -> axum_test::TestResponse {
    app.server
        .post(&format!("/challenges/review/{dataset}/attempt"))
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "problem": problem, "correct": u8::from(correct) }))
        .await
}

async fn counts(app: &TestApp, user_id: i64) -> MistakeCounts {
    let delay = ops::review_delay(&app.pool, user_id).await.unwrap();
    ops::mistake_counts(&app.pool, user_id, Dataset::Ugphysics, delay)
        .await
        .unwrap()
}

async fn rights(app: &TestApp, user_id: i64, problem: i64) -> i64 {
    ops::mistake_state(&app.pool, user_id, problem, 3)
        .await
        .unwrap()
        .unwrap()
        .rights
}

async fn level(pool: &SqlitePool, user_id: i64, subject: &str) -> Option<(i64, i64, bool)> {
    sqlx::query_as(
        "SELECT level, streak, fast_start FROM challenges_progress
         WHERE user_id = ? AND subject = ?",
    )
    .bind(user_id)
    .bind(subject)
    .fetch_optional(pool)
    .await
    .unwrap()
}

fn badge(count: &str) -> String {
    format!(r#"<span id="challenges-review-badge" class="nav-badge">{count}</span>"#)
}

const NO_BADGE: &str = r#"<span id="challenges-review-badge" class="nav-badge" hidden>0</span>"#;

#[tokio::test]
async fn review_requires_authentication() {
    let app = app().await;
    for url in ["/challenges/review", "/challenges/review/ugphysics"] {
        let response = app.server.get(url).expect_failure().await;
        assert_eq!(response.status_code(), 303, "{url}");
    }
}

#[tokio::test]
async fn nav_has_a_review_tab() {
    let app = app().await;
    login(&app).await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains(r#"<a href="/challenges/review">Review"#));
    assert!(body.contains(NO_BADGE));
}

#[tokio::test]
async fn without_mistakes_the_landing_says_so() {
    let app = app().await;
    login(&app).await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains("No mistakes yet."));
    assert!(!body.contains("Start review"));
}

#[tokio::test]
async fn a_wrong_answer_waits_then_comes_due() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "Find the focal length").await;
    ops::record_attempt(&app.pool, user_id, p, false)
        .await
        .unwrap();

    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains("0 due · 1 waiting · 0 cleared"));
    assert!(body.contains("Nothing due. The next mistake comes back on"));
    assert!(!body.contains("Start review"));
    assert!(body.contains(NO_BADGE));

    age(&app.pool, p, 3).await;
    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains("1 due · 0 waiting · 0 cleared"));
    assert!(body.contains(r#"href="/challenges/review/ugphysics" class="btn btn-primary challenges-start">Start review<"#));
    assert!(body.contains(&badge("1")));

    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains("Find the focal length"));
    assert!(body.contains(r#"<p class="challenges-review-context">Marked wrong on "#));
    assert!(body.contains(r#"action="/challenges/review/ugphysics/attempt""#));
    assert!(body.contains(">Not now<"));
    assert!(!body.contains(">Skip<"));
}

#[tokio::test]
async fn each_cooling_off_splits_due_from_waiting() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    ops::record_attempt(&app.pool, user_id, p, false)
        .await
        .unwrap();
    assert_eq!(ops::review_delay(&app.pool, user_id).await.unwrap(), 3);

    // (delay, days since the wrong answer, due?)
    let cases = [
        (0, 0, true),
        (1, 0, false),
        (1, 1, true),
        (3, 2, false),
        (3, 3, true),
        (7, 6, false),
        (7, 7, true),
    ];
    let mut aged = 0;
    for (delay, days, due) in cases {
        age(&app.pool, p, days - aged).await;
        aged = days;
        assert!(
            ops::set_review_delay(&app.pool, user_id, delay)
                .await
                .unwrap()
        );
        let c = counts(&app, user_id).await;
        assert_eq!(
            (c.due, c.waiting),
            if due { (1, 0) } else { (0, 1) },
            "delay {delay}, {days} days old"
        );
    }
}

#[tokio::test]
async fn the_cooling_off_form_stores_one_of_four_values() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    ops::record_attempt(&app.pool, user_id, p, false)
        .await
        .unwrap();

    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains(r#"value="3" aria-pressed="true">3 days<"#));
    assert!(body.contains(r#"value="0" aria-pressed="false">Now<"#));

    // Now makes the waiting mistake due at once, badge included.
    let body = app
        .server
        .post("/challenges/review/delay")
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "days": 0 }))
        .await
        .text();
    assert!(body.starts_with(r#"<div id="challenges-review">"#));
    assert!(body.contains(r#"value="0" aria-pressed="true">Now<"#));
    assert!(body.contains("1 due · 0 waiting"));
    assert!(body.contains(
        r#"<span id="challenges-review-badge" class="nav-badge" hx-swap-oob="true">1</span>"#
    ));
    assert_eq!(ops::review_delay(&app.pool, user_id).await.unwrap(), 0);

    let response = app
        .server
        .post("/challenges/review/delay")
        .form(&serde_json::json!({ "days": 2 }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
    assert_eq!(ops::review_delay(&app.pool, user_id).await.unwrap(), 0);

    // A plain form post goes back to the landing.
    let response = app
        .server
        .post("/challenges/review/delay")
        .form(&serde_json::json!({ "days": 7 }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(ops::review_delay(&app.pool, user_id).await.unwrap(), 7);
}

#[tokio::test]
async fn two_right_replays_clear_a_mistake() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;
    let before = level(&app.pool, user_id, "Optics").await;

    app.server.get("/challenges/review/ugphysics").await;
    let body = replay(&app, "ugphysics", p, true).await.text();
    assert!(body.contains("Right. Once more on"));
    assert!(body.contains("to clear it."));
    assert_eq!(rights(&app, user_id, p).await, 1);
    let c = counts(&app, user_id).await;
    assert_eq!((c.due, c.waiting, c.cleared), (0, 1, 0));
    // Nothing else is due, so the page says the review is done.
    assert!(body.contains("Nothing more due here for now."));

    let is_review: Vec<bool> = sqlx::query_scalar(
        "SELECT is_review FROM challenges_attempts WHERE problem_id = ? ORDER BY id",
    )
    .bind(p)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert_eq!(is_review, [false, true]);

    age(&app.pool, p, 3).await;
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains("· right once since"));
    let body = replay(&app, "ugphysics", p, true).await.text();
    assert!(body.contains("Cleared. Two right in a row."));
    let c = counts(&app, user_id).await;
    assert_eq!((c.due, c.waiting, c.cleared), (0, 0, 1));

    // Replays never moved the staircase.
    assert_eq!(level(&app.pool, user_id, "Optics").await, before);
}

#[tokio::test]
async fn a_wrong_replay_resets_the_count() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;

    app.server.get("/challenges/review/ugphysics").await;
    replay(&app, "ugphysics", p, true).await;
    assert_eq!(rights(&app, user_id, p).await, 1);

    age(&app.pool, p, 3).await;
    app.server.get("/challenges/review/ugphysics").await;
    let body = replay(&app, "ugphysics", p, false).await.text();
    assert!(body.contains("Still open. Back on"));
    assert_eq!(rights(&app, user_id, p).await, 0);
    let c = counts(&app, user_id).await;
    assert_eq!((c.due, c.waiting), (0, 1));
}

#[tokio::test]
async fn a_right_answer_in_practice_counts_and_drops_the_replay() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "Practice repeat").await;
    due_mistake(&app, user_id, p).await;
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains("Practice repeat"));

    // Practice repeats it (as its longest-ago fallback) and you get it right.
    ops::record_attempt(&app.pool, user_id, p, true)
        .await
        .unwrap();
    assert_eq!(rights(&app, user_id, p).await, 1);
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(!body.contains("Practice repeat"));
    assert!(body.contains("Nothing more due here for now."));
}

#[tokio::test]
async fn a_cleared_problem_wrong_again_in_practice_reopens() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    for correct in [false, true, true] {
        ops::record_attempt(&app.pool, user_id, p, correct)
            .await
            .unwrap();
        age(&app.pool, p, 1).await;
    }
    assert_eq!(counts(&app, user_id).await.cleared, 1);

    ops::record_attempt(&app.pool, user_id, p, false)
        .await
        .unwrap();
    let c = counts(&app, user_id).await;
    assert_eq!((c.waiting, c.cleared), (1, 0));
}

#[tokio::test]
async fn replays_stay_out_of_the_practice_stats() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;
    app.server.get("/challenges/review/ugphysics").await;
    replay(&app, "ugphysics", p, true).await;

    let stats = ops::dataset_stats(&app.pool, user_id, Dataset::Ugphysics)
        .await
        .unwrap();
    assert_eq!((stats.total.attempts, stats.total.correct), (1, 0));

    let body = app.server.get("/challenges/stats").await.text();
    assert!(body.contains("Total: <strong>0%</strong> (0/1)"));
    assert!(body.contains("<h3>Mistakes</h3>"));
    assert!(body.contains("0 due · 1 waiting · 0 cleared"));
}

#[tokio::test]
async fn not_now_moves_on_and_the_last_one_stays() {
    let app = app().await;
    let user_id = login(&app).await;
    let older = insert_problem(&app.pool, "ugphysics", "Optics", 1, "Older mistake").await;
    let newer = insert_problem(&app.pool, "ugphysics", "Optics", 1, "Newer mistake").await;
    due_mistake(&app, user_id, newer).await;
    due_mistake(&app, user_id, older).await;
    age(&app.pool, older, 1).await;

    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains("Older mistake"));
    let skip = || {
        app.server
            .post("/challenges/review/ugphysics/skip")
            .add_header("hx-request", "true")
    };
    let body = skip().await.text();
    assert!(body.contains("Newer mistake"));
    let body = skip().await.text();
    assert!(body.contains("Older mistake"));

    // With the other cleared out of the way, Not now keeps the only one.
    ops::record_attempt(&app.pool, user_id, newer, true)
        .await
        .unwrap();
    let body = skip().await.text();
    assert!(body.contains("Older mistake"));
    let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(recorded, 3);
}

#[tokio::test]
async fn a_stale_replay_form_records_nothing() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Optics", 1, "a").await;
    let b = insert_problem(&app.pool, "ugphysics", "Optics", 1, "b").await;
    due_mistake(&app, user_id, a).await;
    due_mistake(&app, user_id, b).await;
    age(&app.pool, a, 1).await;
    app.server.get("/challenges/review/ugphysics").await;

    replay(&app, "ugphysics", b, true).await;
    replay(&app, "ugphysics", a, true).await;
    // A second tap on the same form.
    replay(&app, "ugphysics", a, true).await;
    let reviews: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts WHERE is_review = 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(reviews, 1);
}

#[tokio::test]
async fn retired_problems_never_come_up() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "Retired one").await;
    due_mistake(&app, user_id, p).await;
    sqlx::query("UPDATE challenges_problems SET retired_at = datetime('now') WHERE id = ?")
        .bind(p)
        .execute(&app.pool)
        .await
        .unwrap();

    assert_eq!(counts(&app, user_id).await, MistakeCounts::default());
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(!body.contains("Retired one"));
    assert!(body.contains(NO_BADGE));
}

#[tokio::test]
async fn the_badge_skips_hidden_datasets_and_follows_a_replay() {
    let app = app().await;
    let user_id = login(&app).await;
    let physics = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    let maths = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "m").await;
    due_mistake(&app, user_id, physics).await;
    due_mistake(&app, user_id, maths).await;

    let body = app.server.get("/challenges/stats").await.text();
    assert!(body.contains(&badge("2")));

    ops::set_dataset_hidden(&app.pool, user_id, Dataset::HendrycksMath, true)
        .await
        .unwrap();
    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains(&badge("1")));
    assert!(!body.contains("Hendrycks MATH"));

    app.server.get("/challenges/review/ugphysics").await;
    let body = replay(&app, "ugphysics", physics, true).await.text();
    assert!(body.contains(
        r#"<span id="challenges-review-badge" class="nav-badge" hidden hx-swap-oob="true">0</span>"#
    ));
}

#[tokio::test]
async fn the_picker_links_to_due_mistakes() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "q").await;
    let body = app.server.get("/challenges").await.text();
    assert!(!body.contains("challenges-review-link"));

    due_mistake(&app, user_id, p).await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains(
        r#"<a href="/challenges/review/ugphysics" class="challenges-review-link">Review mistakes (1)</a>"#
    ));
}

#[tokio::test]
async fn replayed_problem_text_is_escaped() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(
        &app.pool,
        "ugphysics",
        "<b>Optics</b>",
        1,
        "<script>alert(1)</script>",
    )
    .await;
    due_mistake(&app, user_id, p).await;
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(!body.contains("<script>alert(1)</script>"));
    assert!(body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!body.contains("<b>Optics</b>"));
}

#[tokio::test]
async fn seeded_review_renders() {
    let app = app().await;
    app.seed_and_login(&ChallengesApp).await;
    for url in [
        "/challenges/review",
        "/challenges/review/ugphysics",
        "/challenges/review/hendrycks-math",
        "/challenges/review/data-science",
    ] {
        let response = app.server.get(url).await;
        assert_eq!(response.status_code(), 200, "{url}");
    }
}

#[tokio::test]
async fn an_unknown_dataset_goes_back_to_the_landing() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .get("/challenges/review/nope")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn command_bar_opens_review() {
    let app = app().await;
    let user_id = login(&app).await;
    let mut params = HashMap::new();
    let result = ops::dispatch(&app.pool, user_id, "review_mistakes", &params, "")
        .await
        .unwrap();
    assert_eq!(result.redirect.as_deref(), Some("/challenges/review"));

    params.insert("dataset".to_string(), serde_json::json!("data-science"));
    let result = ops::dispatch(&app.pool, user_id, "review_mistakes", &params, "")
        .await
        .unwrap();
    assert_eq!(
        result.redirect.as_deref(),
        Some("/challenges/review/data-science")
    );

    params.insert("dataset".to_string(), serde_json::json!("nope"));
    assert!(
        ops::dispatch(&app.pool, user_id, "review_mistakes", &params, "")
            .await
            .is_err()
    );
}

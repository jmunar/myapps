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

async fn spanish(app: &TestApp) {
    app.server
        .post("/settings/language")
        .form(&serde_json::json!({ "language": "es", "redirect": "/challenges/review" }))
        .expect_failure()
        .await;
}

#[tokio::test]
async fn review_renders_in_spanish() {
    let app = app().await;
    let user_id = login(&app).await;
    spanish(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    let m = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "m").await;
    due_mistake(&app, user_id, p).await;
    ops::record_attempt(&app.pool, user_id, m, false)
        .await
        .unwrap();

    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains(r#"<a href="/challenges/review" class="active">Repasar<span"#));
    assert!(body.contains("1 pendientes · 0 en espera · 0 resueltos"));
    assert!(body.contains("0 pendientes · 1 en espera · 0 resueltos"));
    assert!(body.contains(">Empezar repaso<"));
    assert!(body.contains("Nada pendiente. El próximo fallo vuelve el"));
    assert!(body.contains("Volver a mostrar los fallos tras"));
    assert!(body.contains(r#"value="3" aria-pressed="true">3 días<"#));
    assert!(body.contains(r#"value="0" aria-pressed="false">Ya<"#));
    for english in [
        "Start review",
        "due ·",
        "waiting",
        "Bring mistakes back",
        "3 days",
    ] {
        assert!(!body.contains(english), "{english}");
    }

    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains(">Ahora no<"));
    assert!(!body.contains(">Not now<"));
    let body = replay(&app, "ugphysics", p, true).await.text();
    assert!(body.contains("Bien. Otra vez el"));
    assert!(body.contains("y queda resuelto."));
    assert!(body.contains("No queda nada pendiente aquí por ahora."));
    assert!(body.contains(
        r#"<a href="/challenges/practice/ugphysics" class="btn btn-primary">Practicar</a>"#
    ));
    assert!(!body.contains("Right. Once more on"));
}

#[tokio::test]
async fn plain_replay_and_not_now_redirect_to_the_replay_page() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Optics", 1, "a").await;
    let b = insert_problem(&app.pool, "ugphysics", "Optics", 1, "b").await;
    due_mistake(&app, user_id, a).await;
    due_mistake(&app, user_id, b).await;
    age(&app.pool, a, 1).await;
    app.server.get("/challenges/review/ugphysics").await;

    let response = app
        .server
        .post("/challenges/review/ugphysics/skip")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges/review/ugphysics");
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains(&format!(r#"name="problem" value="{b}""#)));

    let response = app
        .server
        .post("/challenges/review/ugphysics/attempt")
        .form(&serde_json::json!({ "problem": b, "correct": 1 }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges/review/ugphysics");
    assert_eq!(rights(&app, user_id, b).await, 1);
    let reviews: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts WHERE is_review = 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(reviews, 1);
}

#[tokio::test]
async fn replay_posts_to_an_unknown_dataset_go_back_to_the_landing() {
    let app = app().await;
    login(&app).await;
    for action in ["skip", "attempt"] {
        let url = format!("/challenges/review/nope/{action}");
        let form = serde_json::json!({ "problem": 1, "correct": 1 });

        let response = app
            .server
            .post(&url)
            .add_header("hx-request", "true")
            .form(&form)
            .await;
        assert_eq!(response.status_code(), 200, "{action}");
        assert_eq!(response.header("hx-redirect"), "/challenges/review");

        let response = app.server.post(&url).form(&form).expect_failure().await;
        assert_eq!(response.status_code(), 303, "{action}");
        assert_eq!(response.header("location"), "/challenges/review");
    }
    let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(recorded, 0);
}

#[tokio::test]
async fn not_now_carries_the_badge() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;
    app.server.get("/challenges/review/ugphysics").await;
    let body = app
        .server
        .post("/challenges/review/ugphysics/skip")
        .add_header("hx-request", "true")
        .await
        .text();
    assert!(body.contains(
        r#"<span id="challenges-review-badge" class="nav-badge" hx-swap-oob="true">1</span>"#
    ));
}

#[tokio::test]
async fn the_done_panel_links_back_and_says_when_next() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    ops::record_attempt(&app.pool, user_id, p, false)
        .await
        .unwrap();

    let response = app.server.get("/challenges/review/ugphysics").await;
    assert_eq!(response.header("cache-control"), "no-store");
    let body = response.text();
    assert!(body.contains(r#"<div id="challenges-practice">"#));
    assert!(body.contains(r#"<div class="card challenges-review-done">"#));
    assert!(body.contains("<h2>Nothing more due here for now.</h2>"));
    assert!(body.contains(
        r#"<p class="text-sm challenges-review-counts">0 due · 1 waiting · 0 cleared</p>"#
    ));
    assert!(body.contains("Nothing due. The next mistake comes back on"));
    assert!(body.contains(r#"<p class="challenges-review-links">"#));
    assert!(body.contains(
        r#"<a href="/challenges/practice/ugphysics" class="btn btn-primary">Practice</a>"#
    ));
    assert!(body.contains(r#"<a href="/challenges/review">Review</a>"#));
    assert!(!body.contains(r#"class="challenges-grade""#));
    // The Review tab is the active one, badge inside it.
    assert!(body.contains(&format!(
        r#"<a href="/challenges/review" class="active">Review{NO_BADGE}</a>"#
    )));
}

#[tokio::test]
async fn the_landing_is_never_cached() {
    let app = app().await;
    login(&app).await;
    let response = app.server.get("/challenges/review").await;
    assert_eq!(response.header("cache-control"), "no-store");
}

#[tokio::test]
async fn replay_forms_post_to_review_and_swap_the_practice_box() {
    let app = app().await;
    let user_id = login(&app).await;
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;
    let body = app.server.get("/challenges/review/ugphysics").await.text();
    assert!(body.contains(r#"<div id="challenges-practice">"#));
    assert!(body.contains(
        r##"hx-post="/challenges/review/ugphysics/attempt" hx-target="#challenges-practice""##
    ));
    assert!(body.contains(
        r##"hx-post="/challenges/review/ugphysics/skip" hx-target="#challenges-practice""##
    ));
    assert!(!body.contains("/challenges/practice/ugphysics/attempt"));
    assert!(!body.contains("/challenges/practice/ugphysics/skip"));
    assert!(body.contains(&badge("1")));
}

#[tokio::test]
async fn the_landing_form_swaps_the_landing() {
    let app = app().await;
    login(&app).await;
    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains(r#"<div id="challenges-review">"#));
    assert!(body.contains(
        r##"hx-post="/challenges/review/delay" hx-target="#challenges-review" hx-swap="outerHTML""##
    ));
    assert!(body.contains(r#"action="/challenges/review/delay""#));
}

/// The result banners, the segmented control and the hidden badge are styled
/// by class and attribute; a rename on either side leaves them unstyled (or,
/// for the badge, visible at zero) with nothing failing.
#[tokio::test]
async fn review_markup_matches_the_stylesheets() {
    let app = app().await;
    let user_id = login(&app).await;
    let apps_css = app.server.get("/static/apps.css").await.text();
    // `ServeDir` serves core.css relative to the cwd, which for an app
    // crate's tests is the crate, so read it from the workspace root.
    let core_css = include_str!("../../../../static/core.css");
    for selector in [
        ".challenges-review-result.challenges-review-cleared",
        ".challenges-review-result.challenges-review-once",
        ".challenges-review-result.challenges-review-open",
        ".challenges-review-done h2",
        ".challenges-review-counts",
        ".challenges-review-links",
        ".challenges-review-context",
        ".challenges-review-link",
        ".challenges-review-delay-label",
        ".challenges-review-delay .challenges-review-delay-options button[aria-pressed=\"true\"]",
        ".challenges-review-delay .challenges-review-delay-options button:hover",
    ] {
        assert!(apps_css.contains(selector), "apps.css lacks {selector}");
    }
    for selector in [".nav-badge {", ".nav-badge[hidden] {"] {
        assert!(core_css.contains(selector), "core.css lacks {selector}");
    }

    let body = app.server.get("/challenges/review").await.text();
    assert!(body.contains(r#"class="challenges-review-delay""#));
    assert!(body.contains(r#"<span class="challenges-review-delay-label">"#));
    assert!(body.contains(r#"<div class="challenges-review-delay-options">"#));
    assert!(body.contains(r#"aria-pressed="true""#));

    // Once, open and cleared, each on its own class.
    let p = insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    due_mistake(&app, user_id, p).await;
    app.server.get("/challenges/review/ugphysics").await;
    let body = replay(&app, "ugphysics", p, true).await.text();
    assert!(body.contains(r#"<p class="challenges-review-result challenges-review-once">"#));
    age(&app.pool, p, 3).await;
    app.server.get("/challenges/review/ugphysics").await;
    let mut body = replay(&app, "ugphysics", p, false).await.text();
    assert!(body.contains(r#"<p class="challenges-review-result challenges-review-open">"#));
    for _ in 0..2 {
        age(&app.pool, p, 3).await;
        app.server.get("/challenges/review/ugphysics").await;
        body = replay(&app, "ugphysics", p, true).await.text();
    }
    assert!(body.contains(r#"<p class="challenges-review-result challenges-review-cleared">"#));
}

#[tokio::test]
async fn practice_marking_carries_the_badge() {
    let app = app().await;
    let user_id = login(&app).await;
    assert!(ops::set_review_delay(&app.pool, user_id, 0).await.unwrap());
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "p").await;
    insert_problem(&app.pool, "ugphysics", "Optics", 1, "q").await;
    app.server.get("/challenges/practice/ugphysics").await;
    let first = crate::current(&app.pool, user_id, "ugphysics")
        .await
        .unwrap();

    // With no cooling-off the wrong answer is due at once.
    let body = crate::mark(&app, "ugphysics", first, false).await.text();
    assert!(body.contains(
        r#"<span id="challenges-review-badge" class="nav-badge" hx-swap-oob="true">1</span>"#
    ));

    let body = app
        .server
        .post("/challenges/practice/ugphysics/skip")
        .add_header("hx-request", "true")
        .await
        .text();
    assert!(body.contains(
        r#"<span id="challenges-review-badge" class="nav-badge" hx-swap-oob="true">1</span>"#
    ));
}

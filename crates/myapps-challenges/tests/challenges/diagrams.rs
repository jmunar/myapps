use crate::{app, current, insert_problem, login, mark};
use myapps_challenges::diagram;

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0L1 1"/></svg>"#;

async fn store(pool: &sqlx::SqlitePool, source: &str) -> String {
    let hash = diagram::hash(source);
    sqlx::query("INSERT INTO challenges_diagrams (hash, svg) VALUES (?, ?)")
        .bind(&hash)
        .bind(SVG)
        .execute(pool)
        .await
        .unwrap();
    hash
}

#[tokio::test]
async fn blocks_become_images_and_missing_ones_a_placeholder() {
    let app = app().await;
    login(&app).await;
    let drawn = store(&app.pool, "draw(unitcircle);").await;
    let id = insert_problem(
        &app.pool,
        "hendrycks-math",
        "Geometry",
        1,
        r#"Find $r$. [asy]draw(unitcircle); label("$r$", (0,0));[/asy] Done."#,
    )
    .await;
    // The problem's block is not `draw(unitcircle);` alone, so it has no
    // rendering; the solution's is exactly that, so it has one.
    sqlx::query("UPDATE challenges_problems SET solution = ? WHERE id = ?")
        .bind("As drawn: [asy]draw(unitcircle);[/asy] So $r = 1$.")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    let body = app
        .server
        .get("/challenges/practice/hendrycks-math")
        .await
        .text();
    assert!(body.contains(&format!(
        r#"<img class="challenges-diagram" src="/challenges/diagrams/{drawn}" alt="Diagram">"#
    )));
    assert!(
        body.contains(r#"<span class="challenges-diagram-missing">(diagram not available)</span>"#)
    );
    // No Asymptote source reaches the page, where KaTeX would read its labels.
    assert!(!body.contains("[asy]"));
    assert!(!body.contains("unitcircle"));
    assert!(body.contains("Find $r$. "));
    assert!(body.contains(" So $r = 1$."));
}

#[tokio::test]
async fn a_diagram_is_served_as_a_sandboxed_immutable_svg() {
    let app = app().await;
    login(&app).await;
    let hash = store(&app.pool, "dot((0,0));").await;

    let response = app
        .server
        .get(&format!("/challenges/diagrams/{hash}"))
        .await;
    response.assert_status_ok();
    assert_eq!(response.header("content-type"), "image/svg+xml");
    assert_eq!(response.header("x-content-type-options"), "nosniff");
    let csp = response.header("content-security-policy");
    let csp = csp.to_str().unwrap();
    assert!(
        csp.contains("default-src 'none'") && csp.contains("sandbox"),
        "{csp}"
    );
    assert!(
        response
            .header("cache-control")
            .to_str()
            .unwrap()
            .contains("immutable")
    );
    assert_eq!(response.text(), SVG);

    let unknown = diagram::hash("nothing");
    for path in [unknown.as_str(), "not-a-hash", &hash.to_uppercase()] {
        let response = app
            .server
            .get(&format!("/challenges/diagrams/{path}"))
            .expect_failure()
            .await;
        assert_eq!(response.status_code(), 404, "{path}");
    }
}

#[tokio::test]
async fn diagrams_require_authentication() {
    let app = app().await;
    let hash = store(&app.pool, "dot((0,0));").await;
    let response = app
        .server
        .get(&format!("/challenges/diagrams/{hash}"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn text_around_a_diagram_is_still_escaped() {
    let app = app().await;
    login(&app).await;
    let hash = store(&app.pool, "dot((0,0));").await;
    insert_problem(
        &app.pool,
        "hendrycks-math",
        "Geometry",
        1,
        "<b>Before</b> [asy]dot((0,0));[/asy] <img src=x onerror=alert(1)> [asy]open",
    )
    .await;

    let body = app
        .server
        .get("/challenges/practice/hendrycks-math")
        .await
        .text();
    assert!(body.contains(&format!(
        r#"&lt;b&gt;Before&lt;/b&gt; <img class="challenges-diagram" src="/challenges/diagrams/{hash}" alt="Diagram"> &lt;img src=x onerror=alert(1)&gt; [asy]open"#
    )));
    assert!(!body.contains("<b>Before</b>"));
    assert!(!body.contains("<img src=x"));
}

#[tokio::test]
async fn the_next_problem_swapped_in_after_marking_shows_spanish_diagrams() {
    let app = app().await;
    let user_id = login(&app).await;
    let hash = store(&app.pool, "draw(unitsquare);").await;
    let first = insert_problem(
        &app.pool,
        "hendrycks-math",
        "Geometry",
        1,
        "[asy]draw(unitsquare);[/asy]",
    )
    .await;
    let second = insert_problem(
        &app.pool,
        "hendrycks-math",
        "Geometry",
        1,
        "[asy]draw(unitsquare);[/asy]",
    )
    .await;
    for id in [first, second] {
        sqlx::query("UPDATE challenges_problems SET solution = ? WHERE id = ?")
            .bind("[asy]label(\"$x$\");[/asy]")
            .bind(id)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    app.server
        .post("/settings/language")
        .form(&serde_json::json!({ "language": "es", "redirect": "/challenges" }))
        .expect_failure()
        .await;

    app.server.get("/challenges/practice/hendrycks-math").await;
    let shown = current(&app.pool, user_id, "hendrycks-math").await.unwrap();
    let other = if shown == first { second } else { first };

    let fragment = mark(&app, "hendrycks-math", shown, false).await.text();
    assert!(fragment.contains(&format!(r#"name="problem" value="{other}""#)));
    assert!(!fragment.contains("<html"));
    assert!(fragment.contains(&format!(
        r#"<img class="challenges-diagram" src="/challenges/diagrams/{hash}" alt="Diagrama">"#
    )));
    assert!(
        fragment.contains(
            r#"<span class="challenges-diagram-missing">(diagrama no disponible)</span>"#
        )
    );
    assert!(!fragment.contains("[asy]"));
    assert!(!fragment.contains("alt=\"Diagram\""));
    assert!(!fragment.contains("(diagram not available)"));
}

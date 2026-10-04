use axum::{
    Extension, Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::Deserialize;

use super::{challenges_nav, html_escape};
use crate::dataset::Dataset;
use crate::i18n::{self, Translations};
use crate::ops::{self, Tally};
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

const MATH_JS: &str = include_str!("../static/challenges-math.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/draw", post(draw))
        .route("/problems/{id}", get(problem))
        .route("/problems/{id}/attempt", post(attempt))
        .route("/stats", get(stats))
}

fn subtitle(dataset: Dataset, t: &Translations) -> &'static str {
    match dataset {
        Dataset::Ugphysics => t.physics,
        Dataset::HendrycksMath => t.maths,
    }
}

fn accuracy(tally: &Tally) -> String {
    if tally.attempts == 0 {
        "—".to_string()
    } else {
        format!(
            "{:.0}%",
            100.0 * tally.correct as f64 / tally.attempts as f64
        )
    }
}

/// The form that draws a problem from `dataset`, as a single button.
fn draw_form(
    base: &str,
    dataset: Dataset,
    exclude: Option<i64>,
    label: &str,
    class: &str,
) -> String {
    let exclude = exclude
        .map(|id| format!(r#"<input type="hidden" name="exclude" value="{id}">"#))
        .unwrap_or_default();
    format!(
        r#"<form method="POST" action="{base}/challenges/draw" class="challenges-draw">
            <input type="hidden" name="dataset" value="{key}">{exclude}
            <button type="submit" class="{class}">{label}</button>
        </form>"#,
        key = dataset.key(),
    )
}

// ── Dataset picker ──────────────────────────────────────────

async fn index(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let mut cards = String::new();
    for dataset in Dataset::ALL {
        let stats = ops::dataset_stats(&state.pool, user_id.0, dataset)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Challenges stats query failed: {e:#}");
                ops::DatasetStats {
                    problems: 0,
                    subjects: vec![],
                    levels: vec![],
                    total: Tally {
                        attempts: 0,
                        correct: 0,
                    },
                }
            });
        let summary = if stats.total.attempts == 0 {
            t.no_attempts.to_string()
        } else {
            format!(
                "{} {} · {}/{}",
                accuracy(&stats.total),
                t.accuracy,
                stats.total.correct,
                stats.total.attempts
            )
        };
        let action = if stats.problems == 0 {
            format!(
                r#"<p class="text-secondary text-sm">{}<code>myapps import --app challenges --dataset {}</code></p>"#,
                t.not_imported,
                dataset.key()
            )
        } else {
            draw_form(base, dataset, None, t.start, "")
        };
        cards.push_str(&format!(
            r#"<div class="card challenges-dataset">
                <div class="card-body">
                    <h2>{name}</h2>
                    <p class="text-secondary">{subtitle}</p>
                    <p class="text-sm">{problems} {problems_lbl} · {subjects} {subjects_lbl} · {summary}</p>
                    {action}
                </div>
            </div>"#,
            name = dataset.name(),
            subtitle = subtitle(dataset, t),
            problems = stats.problems,
            problems_lbl = t.problems,
            subjects_lbl = t.subjects,
            subjects = stats.subjects.len(),
        ));
    }

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="challenges-datasets">{cards}</div>"#,
        title = t.picker_title,
        subtitle = t.picker_subtitle,
    );

    Html(render_page(
        t.picker_title,
        &challenges_nav(base, "practice", lang),
        &body,
        &state.config,
        lang,
    ))
}

// ── Draw ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DrawForm {
    dataset: String,
    exclude: Option<i64>,
}

async fn draw(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<DrawForm>,
) -> Response {
    let base = &state.config.base_path;
    let Some(dataset) = Dataset::from_key(&form.dataset) else {
        return Redirect::to(&format!("{base}/challenges")).into_response();
    };
    redirect_to_next(&state, user_id.0, dataset, form.exclude).await
}

/// Draw a problem and redirect to it, or back to the picker if there is none.
async fn redirect_to_next(
    state: &AppState,
    user_id: i64,
    dataset: Dataset,
    exclude: Option<i64>,
) -> Response {
    let base = &state.config.base_path;
    let mut rng = StdRng::from_os_rng();
    match ops::draw(&state.pool, user_id, dataset, exclude, &mut rng).await {
        Ok(Some(id)) => Redirect::to(&format!("{base}/challenges/problems/{id}")).into_response(),
        Ok(None) => Redirect::to(&format!("{base}/challenges")).into_response(),
        Err(e) => {
            tracing::error!("Challenges draw failed: {e:#}");
            Redirect::to(&format!("{base}/challenges")).into_response()
        }
    }
}

// ── Problem ─────────────────────────────────────────────────

/// Escape third-party text and mark it for KaTeX. `white-space: pre-wrap` on
/// the class keeps the dataset's line breaks.
fn math(text: &str) -> String {
    format!(
        r#"<div class="challenges-text" data-challenges-math>{}</div>"#,
        html_escape(text)
    )
}

/// Answers are bare LaTeX (no delimiters); wrap them so KaTeX picks them up,
/// unless they already carry their own.
fn answer_math(answer: &str) -> String {
    if answer.contains('$') || answer.contains("\\(") || answer.contains("\\[") {
        math(answer)
    } else {
        math(&format!("\\(\\displaystyle {answer}\\)"))
    }
}

async fn problem(
    State(state): State<AppState>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Response {
    let base = &state.config.base_path;
    let sv = &state.config.static_version;
    let t = i18n::t(lang);

    let found = ops::get_problem(&state.pool, id).await.unwrap_or_else(|e| {
        tracing::error!("Challenges problem query failed: {e:#}");
        None
    });
    let Some((p, dataset)) = found.and_then(|p| Dataset::from_key(&p.dataset).map(|d| (p, d)))
    else {
        let body = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.not_found);
        return (
            StatusCode::NOT_FOUND,
            Html(render_page(
                t.picker_title,
                &challenges_nav(base, "practice", lang),
                &body,
                &state.config,
                lang,
            )),
        )
            .into_response();
    };

    let topic = p
        .topic
        .as_deref()
        .map(|t| format!(" · {}", html_escape(t)))
        .unwrap_or_default();
    let unit = p
        .unit
        .as_deref()
        .map(|u| {
            format!(
                r#"<p class="text-secondary text-sm">{}</p>"#,
                html_escape(u)
            )
        })
        .unwrap_or_default();
    let answer = if p.answer.is_empty() {
        String::new()
    } else {
        format!(
            r#"<h3>{answer_lbl}</h3>{answer}{unit}"#,
            answer_lbl = t.answer,
            answer = answer_math(&p.answer),
        )
    };

    let body = format!(
        r#"<link rel="stylesheet" href="{base}/static/katex/katex.min.css?v={sv}">
        <script defer src="{base}/static/katex/katex.min.js?v={sv}"></script>
        <script defer src="{base}/static/katex/auto-render.min.js?v={sv}"></script>
        <div class="page-header">
            <h1>{subject}</h1>
            <p>{dataset_name}{topic} · {level_lbl} {difficulty} {of} {max} ({level_name})</p>
        </div>
        <div class="card challenges-problem">
            <div class="card-body">{problem}</div>
        </div>
        <details class="card challenges-reveal">
            <summary>{show_solution}</summary>
            <div class="card-body">
                {answer}
                <h3>{solution_lbl}</h3>
                {solution}
                <h3>{how}</h3>
                <form method="POST" action="{base}/challenges/problems/{id}/attempt" class="challenges-grade">
                    <button type="submit" name="correct" value="1" class="challenges-right">{right}</button>
                    <button type="submit" name="correct" value="0" class="challenges-wrong">{wrong}</button>
                </form>
            </div>
        </details>
        <div class="challenges-footer">
            {skip}
            <p class="text-secondary text-sm">{source_lbl}: <a href="{url}" rel="noopener">{dataset_name}</a> · {license}</p>
        </div>
        <script>{MATH_JS}</script>"#,
        id = p.id,
        subject = html_escape(&p.subject),
        dataset_name = dataset.name(),
        level_lbl = t.level,
        difficulty = p.difficulty,
        of = t.of,
        max = dataset.max_level(),
        level_name = dataset.level_name(p.difficulty),
        problem = math(&p.problem),
        show_solution = t.show_solution,
        solution_lbl = t.solution,
        solution = math(&p.solution),
        how = t.how_did_it_go,
        right = t.got_it_right,
        wrong = t.got_it_wrong,
        skip = draw_form(base, dataset, Some(p.id), t.skip, "btn-secondary"),
        source_lbl = t.source,
        url = html_escape(&p.source_url),
        license = html_escape(&p.license),
    );

    Html(render_page(
        &format!("{} — {}", t.picker_title, html_escape(&p.subject)),
        &challenges_nav(base, "practice", lang),
        &body,
        &state.config,
        lang,
    ))
    .into_response()
}

#[derive(Deserialize)]
struct AttemptForm {
    correct: u8,
}

async fn attempt(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
    Form(form): Form<AttemptForm>,
) -> Response {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let outcome = match ops::record_attempt(&state.pool, user_id.0, id, form.correct == 1).await {
        Ok(Some(o)) => o,
        Ok(None) => return Redirect::to(&format!("{base}/challenges")).into_response(),
        Err(e) => {
            tracing::error!("Challenges attempt failed: {e:#}");
            return Redirect::to(&format!("{base}/challenges/problems/{id}")).into_response();
        }
    };

    if outcome.after == outcome.before {
        return redirect_to_next(&state, user_id.0, outcome.dataset, Some(id)).await;
    }

    // The level changed: say so before moving on.
    let (headline, class) = if outcome.after > outcome.before {
        (t.level_up, "challenges-up")
    } else {
        (t.level_down, "challenges-down")
    };
    let body = format!(
        r#"<div class="card challenges-level-change {class}">
            <div class="card-body">
                <h2>{headline}</h2>
                <p>{subject}: {level_lbl} {before} → {after} {of} {max}</p>
                {next}
            </div>
        </div>"#,
        subject = html_escape(&outcome.subject),
        level_lbl = t.level,
        before = outcome.before,
        after = outcome.after,
        of = t.of,
        max = outcome.dataset.max_level(),
        next = draw_form(base, outcome.dataset, Some(id), t.next_problem, ""),
    );
    Html(render_page(
        t.picker_title,
        &challenges_nav(base, "practice", lang),
        &body,
        &state.config,
        lang,
    ))
    .into_response()
}

// ── Stats ───────────────────────────────────────────────────

async fn stats(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let mut sections = String::new();
    for dataset in Dataset::ALL {
        let stats = match ops::dataset_stats(&state.pool, user_id.0, dataset).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Challenges stats query failed: {e:#}");
                continue;
            }
        };
        if stats.problems == 0 {
            continue;
        }

        let mut rows = String::new();
        for s in &stats.subjects {
            rows.push_str(&format!(
                r#"<tr>
                    <td class="challenges-subject">{subject}</td>
                    <td class="challenges-level">{level_lbl} {level}</td>
                    <td class="challenges-num">{correct}/{attempts}</td>
                    <td class="challenges-acc">{acc}</td>
                </tr>"#,
                subject = html_escape(&s.subject),
                level_lbl = t.level,
                level = s.level,
                attempts = s.tally.attempts,
                correct = s.tally.correct,
                acc = accuracy(&s.tally),
            ));
        }

        let levels: String = stats
            .levels
            .iter()
            .enumerate()
            .map(|(i, tally)| {
                format!(
                    r#"<li><span class="text-secondary">{name}</span> <strong>{acc}</strong> <span class="text-secondary text-sm">({correct}/{attempts})</span></li>"#,
                    name = dataset.level_name(i as i64 + 1),
                    acc = accuracy(tally),
                    correct = tally.correct,
                    attempts = tally.attempts,
                )
            })
            .collect();

        sections.push_str(&format!(
            r#"<div class="card challenges-stats mt-2">
                <div class="card-header"><h2>{name}</h2><span class="text-sm">{total_lbl}: <strong>{total_acc}</strong> ({total_correct}/{total_attempts})</span></div>
                <table class="table-cards">
                    <thead><tr>
                        <th>{col_subject}</th><th>{col_level}</th><th>{col_correct} / {col_attempts}</th><th>{col_accuracy}</th>
                    </tr></thead>
                    <tbody>{rows}</tbody>
                </table>
                <div class="card-body">
                    <h3>{by_level}</h3>
                    <ul class="challenges-levels">{levels}</ul>
                </div>
            </div>"#,
            name = dataset.name(),
            total_lbl = t.total,
            total_acc = accuracy(&stats.total),
            total_correct = stats.total.correct,
            total_attempts = stats.total.attempts,
            col_subject = t.col_subject,
            col_level = t.col_level,
            col_attempts = t.col_attempts,
            col_correct = t.col_correct,
            col_accuracy = t.col_accuracy,
            by_level = t.by_level,
        ));
    }
    if sections.is_empty() {
        sections = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.no_attempts);
    }

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        {sections}"#,
        title = t.stats_title,
        subtitle = t.stats_subtitle,
    );

    Html(render_page(
        &format!("{} — {}", t.picker_title, t.stats_title),
        &challenges_nav(base, "stats", lang),
        &body,
        &state.config,
        lang,
    ))
}

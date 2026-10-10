use axum::{
    Extension, Form, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::Deserialize;

use super::{challenges_nav, html_escape};
use crate::dataset::Dataset;
use crate::i18n::{self, Translations};
use crate::ops::{self, Outcome, Tally};
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

const MATH_JS: &str = include_str!("../static/challenges-math.js");

/// What the marking and skip forms swap. The practice page keeps one URL per
/// dataset and moves between problems by swapping this in place, so the
/// browser history never holds a problem to go back or forward to.
const PRACTICE_TARGET: &str = "#challenges-practice";

/// What hiding or showing a dataset swaps.
const PICKER_TARGET: &str = "#challenges-picker";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/datasets/{key}/hidden", post(set_hidden))
        .route("/practice/{key}", get(practice))
        .route("/practice/{key}/skip", post(skip))
        .route("/practice/{key}/attempt", post(attempt))
        .route("/problems/{id}", get(problem))
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

fn is_htmx(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request")
}

/// Send the browser to `url`: a 303 for a plain form post, `HX-Redirect` for
/// htmx, which would otherwise follow the 303 itself and swap a whole page
/// into the target.
fn go_to(url: &str, htmx: bool) -> Response {
    if htmx {
        ([("hx-redirect", url)], StatusCode::OK).into_response()
    } else {
        Redirect::to(url).into_response()
    }
}

fn practice_url(base: &str, dataset: Dataset) -> String {
    format!("{base}/challenges/practice/{}", dataset.key())
}

// ── Dataset picker ──────────────────────────────────────────

/// The form that hides a dataset from the picker, or shows it again.
fn hidden_form(base: &str, dataset: Dataset, hide: bool, label: &str) -> String {
    format!(
        r#"<form method="POST" action="{base}/challenges/datasets/{key}/hidden" class="challenges-visibility"
            hx-post="{base}/challenges/datasets/{key}/hidden" hx-target="{target}" hx-swap="outerHTML">
            <input type="hidden" name="hidden" value="{value}">
            <button type="submit" class="btn-secondary">{label}</button>
        </form>"#,
        key = dataset.key(),
        value = u8::from(hide),
        target = PICKER_TARGET,
    )
}

/// The dataset cards plus the list of hidden ones; what hiding swaps.
async fn picker(state: &AppState, user_id: i64, lang: Lang) -> String {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let hidden = ops::hidden_datasets(&state.pool, user_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges hidden datasets query failed: {e:#}");
            vec![]
        });

    let mut cards = String::new();
    for dataset in Dataset::ALL.into_iter().filter(|d| !hidden.contains(d)) {
        let stats = ops::dataset_stats(&state.pool, user_id, dataset)
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
                r#"<p class="text-secondary text-sm">{}<code>myapps-challenges-prep {key}</code>{}<code>myapps import --app challenges --dataset {key}.sqlite</code>.</p>"#,
                t.not_imported,
                t.not_imported_load,
                key = dataset.key()
            )
        } else {
            let in_progress = ops::current(&state.pool, user_id, dataset)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Challenges current problem query failed: {e:#}");
                    None
                })
                .is_some();
            format!(
                r#"<a href="{url}" class="btn btn-primary challenges-start">{label}</a>"#,
                url = practice_url(base, dataset),
                label = if in_progress { t.resume } else { t.start },
            )
        };
        cards.push_str(&format!(
            r#"<div class="card challenges-dataset">
                <div class="card-body">
                    <div class="challenges-dataset-head">
                        <h2>{name}</h2>
                        {hide}
                    </div>
                    <p class="text-secondary">{subtitle}</p>
                    <p class="text-sm">{problems} {problems_lbl} · {subjects} {subjects_lbl} · {summary}</p>
                    {action}
                </div>
            </div>"#,
            name = dataset.name(),
            hide = hidden_form(base, dataset, true, t.hide),
            subtitle = subtitle(dataset, t),
            problems = stats.problems,
            problems_lbl = t.problems,
            subjects_lbl = t.subjects,
            subjects = stats.subjects.len(),
        ));
    }
    if cards.is_empty() {
        cards = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.all_hidden);
    } else {
        cards = format!(r#"<div class="challenges-datasets">{cards}</div>"#);
    }

    // In `Dataset::ALL` order rather than the order they were hidden in.
    let hidden_rows: String = Dataset::ALL
        .into_iter()
        .filter(|d| hidden.contains(d))
        .map(|dataset| {
            format!(
                r#"<li><div><strong>{name}</strong> <span class="text-secondary text-sm">{subtitle}</span></div>{show}</li>"#,
                name = dataset.name(),
                subtitle = subtitle(dataset, t),
                show = hidden_form(base, dataset, false, t.show),
            )
        })
        .collect();
    let hidden_section = if hidden_rows.is_empty() {
        String::new()
    } else {
        format!(
            r#"<h2 class="challenges-hidden-title">{title}</h2>
            <ul class="card challenges-hidden">{hidden_rows}</ul>"#,
            title = t.hidden_datasets,
        )
    };

    format!(r#"<div id="challenges-picker">{cards}{hidden_section}</div>"#)
}

async fn index(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        {picker}"#,
        title = t.picker_title,
        subtitle = t.picker_subtitle,
        picker = picker(&state, user_id.0, lang).await,
    );

    (
        // Back from a problem must show Continue rather than the cached Start.
        [(header::CACHE_CONTROL, "no-store")],
        Html(render_page(
            t.picker_title,
            &challenges_nav(base, "practice", lang),
            &body,
            &state.config,
            lang,
        )),
    )
}

#[derive(Deserialize)]
struct HiddenForm {
    hidden: u8,
}

async fn set_hidden(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Form(form): Form<HiddenForm>,
) -> Response {
    if let Some(dataset) = Dataset::from_key(&key)
        && let Err(e) =
            ops::set_dataset_hidden(&state.pool, user_id.0, dataset, form.hidden == 1).await
    {
        tracing::error!("Challenges hide dataset failed: {e:#}");
    }
    if is_htmx(&headers) {
        Html(picker(&state, user_id.0, lang).await).into_response()
    } else {
        Redirect::to(&format!("{}/challenges", state.config.base_path)).into_response()
    }
}

// ── Practice ────────────────────────────────────────────────

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

/// The notice above the next problem when an answer moved your level.
fn level_change(outcome: &Outcome, t: &Translations) -> String {
    if outcome.after == outcome.before {
        return String::new();
    }
    let (headline, class) = if outcome.after > outcome.before {
        (t.level_up, "challenges-up")
    } else {
        (t.level_down, "challenges-down")
    };
    format!(
        r#"<div class="card challenges-level-change {class}">
            <div class="card-body">
                <h2>{headline}</h2>
                <p>{subject}: {level_lbl} {before} → {after} {of} {max}</p>
            </div>
        </div>"#,
        subject = html_escape(&outcome.subject),
        level_lbl = t.level,
        before = outcome.before,
        after = outcome.after,
        of = t.of,
        max = outcome.dataset.max_level(),
    )
}

/// The problem, its hidden solution and the marking and skip forms: what
/// `PRACTICE_TARGET` holds. `None` if the problem does not exist.
async fn problem_fragment(
    state: &AppState,
    lang: Lang,
    problem_id: i64,
    notice: &str,
) -> Option<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let found = ops::get_problem(&state.pool, problem_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges problem query failed: {e:#}");
            None
        });
    let (p, dataset) = found.and_then(|p| Dataset::from_key(&p.dataset).map(|d| (p, d)))?;

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
    let url = practice_url(base, dataset);

    Some(format!(
        r#"{notice}
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
                <form method="POST" action="{url}/attempt" class="challenges-grade"
                    hx-post="{url}/attempt" hx-target="{target}" hx-swap="innerHTML show:window:top">
                    <input type="hidden" name="problem" value="{id}">
                    <button type="submit" name="correct" value="1" class="challenges-right">{right}</button>
                    <button type="submit" name="correct" value="0" class="challenges-wrong">{wrong}</button>
                </form>
            </div>
        </details>
        <div class="challenges-footer">
            <form method="POST" action="{url}/skip" class="challenges-skip"
                hx-post="{url}/skip" hx-target="{target}" hx-swap="innerHTML show:window:top">
                <button type="submit" class="btn-secondary">{skip}</button>
            </form>
            <p class="text-secondary text-sm">{source_lbl}: <a href="{source_url}" rel="noopener">{dataset_name}</a> · {license}</p>
        </div>"#,
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
        target = PRACTICE_TARGET,
        skip = t.skip,
        source_lbl = t.source,
        source_url = html_escape(&p.source_url),
        license = html_escape(&p.license),
    ))
}

/// The practice page for `dataset`: always the problem in progress there,
/// drawing one if there is none.
async fn practice(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(key): Path<String>,
) -> Response {
    let base = &state.config.base_path;
    let sv = &state.config.static_version;
    let t = i18n::t(lang);
    let picker_url = format!("{base}/challenges");

    let Some(dataset) = Dataset::from_key(&key) else {
        return Redirect::to(&picker_url).into_response();
    };
    let mut rng = StdRng::from_os_rng();
    let id = match ops::current_or_draw(&state.pool, user_id.0, dataset, &mut rng).await {
        Ok(Some(id)) => id,
        Ok(None) => return Redirect::to(&picker_url).into_response(),
        Err(e) => {
            tracing::error!("Challenges draw failed: {e:#}");
            return Redirect::to(&picker_url).into_response();
        }
    };
    let Some(fragment) = problem_fragment(&state, lang, id, "").await else {
        return Redirect::to(&picker_url).into_response();
    };

    let body = format!(
        r#"<link rel="stylesheet" href="{base}/static/katex/katex.min.css?v={sv}">
        <script defer src="{base}/static/katex/katex.min.js?v={sv}"></script>
        <script defer src="{base}/static/katex/auto-render.min.js?v={sv}"></script>
        <div id="challenges-practice">{fragment}</div>
        <script>{MATH_JS}</script>"#,
    );

    (
        // The page changes under a fixed URL, so a copy restored from the
        // back-forward cache could show a problem that has since moved on.
        [(header::CACHE_CONTROL, "no-store")],
        Html(render_page(
            &format!("{} — {}", t.picker_title, dataset.name()),
            &challenges_nav(base, "practice", lang),
            &body,
            &state.config,
            lang,
        )),
    )
        .into_response()
}

/// Answer a marking or skip form: the new problem's fragment for htmx, or a
/// 303 back to the practice page for a plain form post.
async fn after_change(
    state: &AppState,
    user_id: i64,
    lang: Lang,
    dataset: Dataset,
    notice: &str,
    htmx: bool,
) -> Response {
    let base = &state.config.base_path;
    if !htmx {
        return Redirect::to(&practice_url(base, dataset)).into_response();
    }
    let current = ops::current(&state.pool, user_id, dataset)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges current problem query failed: {e:#}");
            None
        });
    match current {
        Some(id) => match problem_fragment(state, lang, id, notice).await {
            Some(fragment) => Html(fragment).into_response(),
            None => go_to(&format!("{base}/challenges"), true),
        },
        None => go_to(&practice_url(base, dataset), true),
    }
}

async fn skip(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let base = &state.config.base_path;
    let htmx = is_htmx(&headers);
    let Some(dataset) = Dataset::from_key(&key) else {
        return go_to(&format!("{base}/challenges"), htmx);
    };
    let mut rng = StdRng::from_os_rng();
    if let Err(e) = ops::skip(&state.pool, user_id.0, dataset, &mut rng).await {
        tracing::error!("Challenges skip failed: {e:#}");
    }
    after_change(&state, user_id.0, lang, dataset, "", htmx).await
}

#[derive(Deserialize)]
struct AttemptForm {
    problem: i64,
    correct: u8,
}

async fn attempt(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Form(form): Form<AttemptForm>,
) -> Response {
    let base = &state.config.base_path;
    let htmx = is_htmx(&headers);
    let Some(dataset) = Dataset::from_key(&key) else {
        return go_to(&format!("{base}/challenges"), htmx);
    };
    let mut rng = StdRng::from_os_rng();
    // A stale form (`None`) records nothing and just shows the problem that is
    // current now.
    let notice = match ops::mark(
        &state.pool,
        user_id.0,
        dataset,
        form.problem,
        form.correct == 1,
        &mut rng,
    )
    .await
    {
        Ok(Some(outcome)) => level_change(&outcome, i18n::t(lang)),
        Ok(None) => String::new(),
        Err(e) => {
            tracing::error!("Challenges attempt failed: {e:#}");
            String::new()
        }
    };
    after_change(&state, user_id.0, lang, dataset, &notice, htmx).await
}

/// Problem pages used to have a URL each. Old links and history entries land
/// on the dataset's practice page, which shows the problem in progress there
/// rather than the one linked.
async fn problem(
    State(state): State<AppState>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Response {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let found = ops::get_problem(&state.pool, id).await.unwrap_or_else(|e| {
        tracing::error!("Challenges problem query failed: {e:#}");
        None
    });
    match found.and_then(|p| Dataset::from_key(&p.dataset)) {
        Some(dataset) => Redirect::to(&practice_url(base, dataset)).into_response(),
        None => {
            let body = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.not_found);
            (
                StatusCode::NOT_FOUND,
                Html(render_page(
                    t.picker_title,
                    &challenges_nav(base, "practice", lang),
                    &body,
                    &state.config,
                    lang,
                )),
            )
                .into_response()
        }
    }
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

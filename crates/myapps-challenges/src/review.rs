//! The Review tab: problems you got wrong, back after your cooling-off, until
//! two right answers in a row clear them. The queue is `ops`'s; this is its
//! landing page, the cooling-off setting and the replay page, which reuses
//! the practice page's problem fragment.

use axum::{
    Extension, Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::dataset::Dataset;
use crate::i18n::{self, Translations};
use crate::ops::{self, MistakeCounts, Mode, ReviewOutcome};
use crate::pages::{
    MATH_JS, badge_oob, day, go_to, is_htmx, nav, problem_fragment, review_url, subtitle,
};
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

/// What changing the cooling-off swaps: the whole landing, since it moves
/// mistakes between due and waiting.
const LANDING_TARGET: &str = "#challenges-review";

/// "4 due · 2 waiting · 11 cleared".
pub(crate) fn counts_line(c: &MistakeCounts, t: &Translations) -> String {
    format!(
        "{} {} · {} {} · {} {}",
        c.due, t.review_due, c.waiting, t.review_waiting, c.cleared, t.review_cleared
    )
}

/// What to say under a dataset's counts when nothing there is due.
fn nothing_due(c: &MistakeCounts, t: &Translations) -> String {
    match &c.next_due_on {
        Some(on) => format!(
            r#"<p class="text-secondary text-sm">{} {}.</p>"#,
            t.review_nothing_due,
            day(on, t)
        ),
        None => String::new(),
    }
}

async fn delay(state: &AppState, user_id: i64) -> i64 {
    ops::review_delay(&state.pool, user_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges review delay query failed: {e:#}");
            ops::DEFAULT_REVIEW_DELAY
        })
}

async fn counts(state: &AppState, user_id: i64, dataset: Dataset, delay: i64) -> MistakeCounts {
    ops::mistake_counts(&state.pool, user_id, dataset, delay)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges mistake counts query failed: {e:#}");
            MistakeCounts::default()
        })
}

// ── Landing ─────────────────────────────────────────────────

/// One card per visible dataset that has been loaded, then the cooling-off
/// control; what `LANDING_TARGET` holds.
async fn landing_fragment(state: &AppState, user_id: i64, lang: Lang) -> String {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let delay = delay(state, user_id).await;
    let hidden = ops::hidden_datasets(&state.pool, user_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges hidden datasets query failed: {e:#}");
            vec![]
        });

    let mut cards = String::new();
    for dataset in Dataset::ALL.into_iter().filter(|d| !hidden.contains(d)) {
        let c = counts(state, user_id, dataset, delay).await;
        let body = if c == MistakeCounts::default() {
            format!(
                r#"<p class="text-secondary text-sm">{}</p>"#,
                t.review_no_mistakes
            )
        } else {
            let action = if c.due > 0 {
                format!(
                    r#"<a href="{url}" class="btn btn-primary challenges-start">{label}</a>"#,
                    url = review_url(base, dataset),
                    label = t.review_start,
                )
            } else {
                nothing_due(&c, t)
            };
            format!(
                r#"<p class="text-sm challenges-review-counts">{}</p>{action}"#,
                counts_line(&c, t)
            )
        };
        cards.push_str(&format!(
            r#"<div class="card challenges-dataset">
                <div class="card-body">
                    <h2>{name}</h2>
                    <p class="text-secondary">{subtitle}</p>
                    {body}
                </div>
            </div>"#,
            name = dataset.name(),
            subtitle = subtitle(dataset, t),
        ));
    }
    if cards.is_empty() {
        cards = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.all_hidden);
    } else {
        cards = format!(r#"<div class="challenges-datasets">{cards}</div>"#);
    }

    let options: String = ops::REVIEW_DELAYS
        .iter()
        .zip(t.review_delays)
        .map(|(&days, label)| {
            format!(
                r#"<button type="submit" name="days" value="{days}" aria-pressed="{pressed}">{label}</button>"#,
                pressed = days == delay,
            )
        })
        .collect();

    format!(
        r#"<div id="challenges-review">
            {cards}
            <form method="POST" action="{base}/challenges/review/delay" class="challenges-review-delay"
                hx-post="{base}/challenges/review/delay" hx-target="{target}" hx-swap="outerHTML">
                <span class="challenges-review-delay-label">{label}</span>
                <div class="challenges-review-delay-options">{options}</div>
            </form>
        </div>"#,
        label = t.review_delay_label,
        target = LANDING_TARGET,
    )
}

pub(crate) async fn landing(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> impl IntoResponse {
    let t = i18n::t(lang);
    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        {landing}"#,
        title = t.review_title,
        subtitle = t.review_subtitle,
        landing = landing_fragment(&state, user_id.0, lang).await,
    );
    (
        // Back from a replay must show the counts as they are now.
        [(header::CACHE_CONTROL, "no-store")],
        Html(render_page(
            &format!("{} — {}", t.picker_title, t.review_title),
            &nav(&state, user_id.0, "review", lang).await,
            &body,
            &state.config,
            lang,
        )),
    )
}

#[derive(Deserialize)]
pub(crate) struct DelayForm {
    days: i64,
}

pub(crate) async fn set_delay(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Form(form): Form<DelayForm>,
) -> Response {
    match ops::set_review_delay(&state.pool, user_id.0, form.days).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::BAD_REQUEST.into_response(),
        Err(e) => tracing::error!("Challenges set review delay failed: {e:#}"),
    }
    if is_htmx(&headers) {
        let fragment = landing_fragment(&state, user_id.0, lang).await;
        Html(fragment + &badge_oob(&state, user_id.0).await).into_response()
    } else {
        Redirect::to(&format!("{}/challenges/review", state.config.base_path)).into_response()
    }
}

// ── Replaying ───────────────────────────────────────────────

/// The one-line result of a replay, above the next mistake.
fn result_banner(outcome: &ReviewOutcome, t: &Translations) -> String {
    let (class, text) = if !outcome.correct {
        (
            "challenges-review-open",
            format!("{} {}.", t.result_open, day(&outcome.state.due_on, t)),
        )
    } else if outcome.state.cleared() {
        ("challenges-review-cleared", t.result_cleared.to_string())
    } else {
        (
            "challenges-review-once",
            format!(
                "{} {} {}",
                t.result_once,
                day(&outcome.state.due_on, t),
                t.result_once_after
            ),
        )
    };
    format!(r#"<p class="challenges-review-result {class}">{text}</p>"#)
}

/// What the replay page shows when nothing in `dataset` is due.
async fn done_fragment(
    state: &AppState,
    user_id: i64,
    lang: Lang,
    dataset: Dataset,
    notice: &str,
) -> String {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let delay = delay(state, user_id).await;
    let c = counts(state, user_id, dataset, delay).await;
    format!(
        r#"{notice}
        <div class="page-header">
            <h1>{name}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card challenges-review-done">
            <div class="card-body">
                <h2>{done}</h2>
                <p class="text-sm challenges-review-counts">{counts}</p>
                {next}
                <p class="challenges-review-links">
                    <a href="{base}/challenges/practice/{key}" class="btn btn-primary">{practice}</a>
                    <a href="{base}/challenges/review">{review}</a>
                </p>
            </div>
        </div>"#,
        name = dataset.name(),
        subtitle = subtitle(dataset, t),
        done = t.review_done,
        counts = counts_line(&c, t),
        next = nothing_due(&c, t),
        key = dataset.key(),
        practice = t.nav_practice,
        review = t.review_title,
    )
}

/// The mistake being replayed in `dataset`, or the done panel; what
/// the replay page's `#challenges-practice` holds, as on the practice page.
async fn current_fragment(
    state: &AppState,
    user_id: i64,
    lang: Lang,
    dataset: Dataset,
    notice: &str,
) -> String {
    let current = ops::review_current_or_next(&state.pool, user_id, dataset)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Challenges review draw failed: {e:#}");
            None
        });
    if let Some(id) = current
        && let Some(fragment) =
            problem_fragment(state, lang, user_id, id, notice, Mode::Review).await
    {
        return fragment;
    }
    done_fragment(state, user_id, lang, dataset, notice).await
}

pub(crate) async fn page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(key): Path<String>,
) -> Response {
    let base = &state.config.base_path;
    let sv = &state.config.static_version;
    let t = i18n::t(lang);
    let Some(dataset) = Dataset::from_key(&key) else {
        return Redirect::to(&format!("{base}/challenges/review")).into_response();
    };
    let fragment = current_fragment(&state, user_id.0, lang, dataset, "").await;
    let body = format!(
        r#"<link rel="stylesheet" href="{base}/static/katex/katex.min.css?v={sv}">
        <script defer src="{base}/static/katex/katex.min.js?v={sv}"></script>
        <script defer src="{base}/static/katex/auto-render.min.js?v={sv}"></script>
        <div id="challenges-practice">{fragment}</div>
        <script>{MATH_JS}</script>"#,
    );
    (
        // Like practice: the page changes under a fixed URL.
        [(header::CACHE_CONTROL, "no-store")],
        Html(render_page(
            &format!(
                "{} — {} — {}",
                t.picker_title,
                t.review_title,
                dataset.name()
            ),
            &nav(&state, user_id.0, "review", lang).await,
            &body,
            &state.config,
            lang,
        )),
    )
        .into_response()
}

/// Answer a replay or Not now: the next mistake's fragment and the badge for
/// htmx, or a 303 back to the replay page for a plain form post.
async fn after_change(
    state: &AppState,
    user_id: i64,
    lang: Lang,
    dataset: Dataset,
    notice: &str,
    htmx: bool,
) -> Response {
    if !htmx {
        return Redirect::to(&review_url(&state.config.base_path, dataset)).into_response();
    }
    let fragment = current_fragment(state, user_id, lang, dataset, notice).await;
    Html(fragment + &badge_oob(state, user_id).await).into_response()
}

pub(crate) async fn skip(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let htmx = is_htmx(&headers);
    let Some(dataset) = Dataset::from_key(&key) else {
        return go_to(
            &format!("{}/challenges/review", state.config.base_path),
            htmx,
        );
    };
    if let Err(e) = ops::review_skip(&state.pool, user_id.0, dataset).await {
        tracing::error!("Challenges review skip failed: {e:#}");
    }
    after_change(&state, user_id.0, lang, dataset, "", htmx).await
}

#[derive(Deserialize)]
pub(crate) struct AttemptForm {
    problem: i64,
    correct: u8,
}

pub(crate) async fn attempt(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Form(form): Form<AttemptForm>,
) -> Response {
    let htmx = is_htmx(&headers);
    let Some(dataset) = Dataset::from_key(&key) else {
        return go_to(
            &format!("{}/challenges/review", state.config.base_path),
            htmx,
        );
    };
    // A stale form (`None`) records nothing and just shows what is current.
    let notice = match ops::review_mark(
        &state.pool,
        user_id.0,
        dataset,
        form.problem,
        form.correct == 1,
    )
    .await
    {
        Ok(Some(outcome)) => result_banner(&outcome, i18n::t(lang)),
        Ok(None) => String::new(),
        Err(e) => {
            tracing::error!("Challenges review attempt failed: {e:#}");
            String::new()
        }
    };
    after_change(&state, user_id.0, lang, dataset, &notice, htmx).await
}

//! Breakdown: spending inside one label group, over time and by category.
//!
//! Two linked charts share a single payload, so picking a time bucket re-cuts
//! the category chart without another round trip: a time series of the group's
//! total per bucket (day / week / month), and a horizontal bar per category in
//! the group, for the selected bucket or the whole window. Clicking a category
//! bar lists the transactions behind it.
//!
//! The window comes from `period`: whole calendar periods, plus the running one
//! when the selector's `+` is on. Every bucket carries a weight, so the charts
//! can show a per-period average that the half-finished current period does not
//! distort.

use axum::{
    Extension, Router,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;

use super::colors::group_color;
use super::dashboard::leanfin_nav;
use super::labels::{GroupRow, group_display_name, user_groups};
use super::period::{self, Bucket, Window};
use super::services::expenses;
use myapps_core::auth::UserId;
use myapps_core::components::html_escape;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

/// Chart wiring for this page. Kept in its own file so the Rust side never has
/// to brace-escape a page of JavaScript.
const BREAKDOWN_JS: &str = include_str!("../static/breakdown.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/breakdown", get(page))
        .route("/breakdown/chart", get(chart_data))
        // The tab was called Expenses until the group breakdown replaced it.
        .route("/expenses", get(legacy_redirect))
}

async fn legacy_redirect(state: axum::extract::State<AppState>) -> impl IntoResponse {
    Redirect::permanent(&format!("{}/leanfin/breakdown", state.config.base_path))
}

/// Serialize for embedding in a `<script>` body. `serde_json` handles quotes and
/// control characters; escaping the three markup characters on top of that keeps
/// a category named `</script>` from closing the element.
fn json_for_script(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// Name of the per-period average, which depends on how wide a bucket is.
fn average_label(bucket: Bucket, lang: Lang) -> &'static str {
    let t = super::i18n::t(lang);
    match bucket {
        Bucket::Day => t.exp_avg_day,
        Bucket::Week => t.exp_avg_week,
        Bucket::Month => t.exp_avg_month,
    }
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let label_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_labels WHERE user_id = ?")
            .bind(user_id.0)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    if label_count == 0 {
        let body = format!(
            r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="empty-state"><p>{no_labels}</p></div>
        </div>"#,
            title = t.exp_title,
            subtitle = t.exp_subtitle,
            no_labels = t.exp_no_labels,
        );
        return Html(render_page(
            &format!("LeanFin — {}", t.expenses),
            &leanfin_nav(base, "breakdown", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let groups: Vec<GroupRow> = user_groups(&state.pool, user_id.0)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        });

    let mut group_pills = String::new();
    for g in &groups {
        let color = group_color(g.id);
        group_pills.push_str(&format!(
            r##"<button type="button" class="label-badge label-pill" style="--label-color:{color}"
                    data-group-id="{id}">{name}</button> "##,
            id = g.id,
            name = html_escape(&group_display_name(&g.name, lang)),
        ));
    }

    let selector = period::render_selector(
        period::DEFAULT_WINDOW,
        period::DEFAULT_INCLUDE_CURRENT,
        "breakdownWindowChanged",
        lang,
    );

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="expenses-controls" id="breakdown-controls"
                 data-base="{base}"
                 data-window="{window}"
                 data-current="{current}"
                 data-msg-select-group="{select_group_attr}"
                 data-msg-full-range="{full_range_attr}">
                <div class="expenses-labels">
                    {group_pills}
                </div>
                {selector}
            </div>
            <div class="chart-container" id="breakdown-chart-container" style="display:none"><canvas id="breakdown-canvas"></canvas></div>
            <div id="breakdown-empty" class="empty-state"><p>{select_group}</p></div>
            <div id="breakdown-data"></div>
        </div>
        <div class="card mt-2" id="breakdown-categories-card" style="display:none">
            <div class="card-header">
                <h2>{by_category}</h2>
                <span id="breakdown-range" class="text-sm text-secondary"></span>
            </div>
            <div class="chart-container" id="breakdown-categories-container">
                <canvas id="breakdown-categories-canvas"></canvas>
            </div>
        </div>
        <div class="card mt-2" id="breakdown-txn-card" style="display:none">
            <div class="card-header">
                <h2>{transactions}</h2>
                <span id="breakdown-txn-range" class="text-sm text-secondary"></span>
            </div>
            <div id="breakdown-txn-table"></div>
        </div>
        <script>{selector_js}</script>
        <script>{breakdown_js}</script>"##,
        title = t.exp_title,
        subtitle = t.exp_subtitle,
        select_group = t.exp_select_group,
        by_category = t.exp_by_category,
        transactions = t.exp_transactions,
        window = period::DEFAULT_WINDOW.key(),
        current = if period::DEFAULT_INCLUDE_CURRENT {
            "1"
        } else {
            "0"
        },
        select_group_attr = html_escape(t.exp_select_group),
        full_range_attr = html_escape(t.exp_full_range),
        selector_js = period::SELECTOR_JS,
        breakdown_js = BREAKDOWN_JS,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.expenses),
        &leanfin_nav(base, "breakdown", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct ChartQuery {
    group_id: i64,
    /// Both of these reach the server from a URL a person can edit, so neither
    /// rejects a bad value — they fall back to the selector's own defaults.
    #[serde(default)]
    window: Option<String>,
    #[serde(default)]
    current: Option<String>,
}

fn include_current(raw: Option<&str>) -> bool {
    match raw {
        Some(v) => v == "1" || v == "true",
        None => period::DEFAULT_INCLUDE_CURRENT,
    }
}

async fn chart_data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<ChartQuery>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    let group: Option<GroupRow> = sqlx::query_as(
        "SELECT id, name, is_default FROM leanfin_label_groups WHERE id = ? AND user_id = ?",
    )
    .bind(params.group_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(group) = group else {
        return Html(empty_script(t.exp_group_not_found));
    };

    let labels: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM leanfin_labels WHERE user_id = ? AND group_id = ? ORDER BY name COLLATE NOCASE",
    )
    .bind(user_id.0)
    .bind(group.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if labels.is_empty() {
        return Html(empty_script(t.exp_group_empty));
    }

    let window = Window::parse(params.window.as_deref());
    let today = chrono::Utc::now().date_naive();
    let periods = period::periods(window, include_current(params.current.as_deref()), today);
    let window_start = periods.start();
    let window_end = periods
        .periods
        .last()
        .map(|p| p.end)
        .unwrap_or(window_start);

    let label_ids: Vec<i64> = labels.iter().map(|(id, _)| *id).collect();
    let raw = expenses::get_expense_series(
        &state.pool,
        user_id.0,
        &label_ids,
        &period::iso(window_start),
        &period::iso(window_end),
    )
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if raw.is_empty() {
        return Html(empty_script(t.exp_no_data));
    }

    // (bucket end, label) → total, so the matrix below is a plain lookup. The
    // daily rows are folded into buckets here rather than in SQL, because only
    // the window knows where a bucket ends.
    let mut data_map: HashMap<(String, i64), f64> = HashMap::new();
    for p in &raw {
        let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") else {
            continue;
        };
        let end = period::iso(period::bucket_end_for(periods.bucket, d));
        *data_map.entry((end, p.label_id)).or_default() += p.total;
    }

    let all_dates = periods.ends();
    let matrix: Vec<Vec<f64>> = label_ids
        .iter()
        .map(|lid| {
            all_dates
                .iter()
                .map(|d| data_map.get(&(d.clone(), *lid)).copied().unwrap_or(0.0))
                .collect()
        })
        .collect();

    let payload = serde_json::json!({
        "groupId": group.id,
        "groupName": group_display_name(&group.name, lang),
        "color": group_color(group.id),
        "dates": all_dates,
        "starts": periods.starts(),
        "weights": periods.weights(),
        "windowStart": period::iso(window_start),
        "avgLabel": average_label(periods.bucket, lang),
        "categories": labels
            .iter()
            .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
            .collect::<Vec<_>>(),
        "matrix": matrix,
    });

    Html(format!(
        "<script>window.updateBreakdown({});</script>",
        json_for_script(&payload)
    ))
}

fn empty_script(message: &str) -> String {
    format!(
        "<script>window.showBreakdownEmpty({});</script>",
        json_for_script(&serde_json::Value::from(message))
    )
}

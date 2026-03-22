use axum::{
    Extension, Form, Router,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::mindflow_nav;
use crate::auth::UserId;
use crate::i18n::Lang;
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/inbox", get(list))
        .route("/inbox/recategorize", post(bulk_recategorize))
}

#[derive(sqlx::FromRow)]
struct InboxThought {
    id: i64,
    content: String,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct CategoryOption {
    id: i64,
    name: String,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let thoughts: Vec<InboxThought> = sqlx::query_as(
        r#"SELECT id, content, created_at
           FROM mindflow_thoughts
           WHERE user_id = ? AND category_id IS NULL AND status = 'active'
           ORDER BY created_at DESC"#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let categories: Vec<CategoryOption> = sqlx::query_as(
        "SELECT id, name FROM mindflow_categories WHERE user_id = ? AND archived = 0 ORDER BY name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut cat_options = String::new();
    for c in &categories {
        cat_options.push_str(&format!(r#"<option value="{}">{}</option>"#, c.id, c.name,));
    }

    let mut rows = String::new();
    for th in &thoughts {
        rows.push_str(&format!(
            r##"<div class="thought-card">
                <label class="thought-checkbox">
                    <input type="checkbox" name="thought_ids" value="{id}" form="bulk-form">
                </label>
                <a href="{base}/mindflow/thoughts/{id}" class="thought-content">{content}</a>
                <span class="text-sm text-secondary">{created_at}</span>
            </div>"##,
            id = th.id,
            content = th.content,
            created_at = th.created_at,
        ));
    }

    if rows.is_empty() {
        rows = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.mf_inbox_empty
        );
    }

    let bulk_bar = if !thoughts.is_empty() && !categories.is_empty() {
        format!(
            r#"<form id="bulk-form" method="POST" action="{base}/mindflow/inbox/recategorize"
                  class="form-row" style="margin-bottom:1rem">
                <select name="category_id" required>
                    <option value="" disabled selected>{move_to}</option>
                    {cat_options}
                </select>
                <button type="submit" class="btn btn-primary btn-sm">{move_selected}</button>
            </form>"#,
            move_to = t.mf_inbox_move_to,
            move_selected = t.mf_inbox_move_selected,
        )
    } else {
        String::new()
    };

    let count = thoughts.len();
    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{count} uncategorized thoughts</p>
        </div>
        <div class="card">
            <div class="card-body">
                {bulk_bar}
                {rows}
            </div>
        </div>"#,
        title = t.mf_inbox_title,
    );

    Html(render_page(
        &format!("MindFlow \u{2014} {}", t.mf_inbox),
        &mindflow_nav(base, "inbox", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct BulkRecategorizeForm {
    category_id: i64,
    thought_ids: Option<String>,
}

async fn bulk_recategorize(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<BulkRecategorizeForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Parse thought IDs (may be comma-separated or multiple form values)
    let ids: Vec<i64> = form
        .thought_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if !ids.is_empty() {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE mindflow_thoughts SET category_id = ?, updated_at = datetime('now') WHERE id IN ({placeholders}) AND user_id = ?"
        );
        let mut query = sqlx::query(&sql).bind(form.category_id);
        for id in &ids {
            query = query.bind(id);
        }
        query.bind(user_id.0).execute(&state.pool).await.ok();
    }

    Redirect::to(&format!("{base}/mindflow/inbox"))
}

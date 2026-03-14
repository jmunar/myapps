use axum::{
    Extension, Form, Router,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::mindflow_nav;
use crate::auth::UserId;
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
) -> Html<String> {
    let base = &state.config.base_path;

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
    for t in &thoughts {
        rows.push_str(&format!(
            r##"<div class="thought-card">
                <label class="thought-checkbox">
                    <input type="checkbox" name="thought_ids" value="{id}" form="bulk-form">
                </label>
                <a href="{base}/mindflow/thoughts/{id}" class="thought-content">{content}</a>
                <span class="text-sm text-secondary">{created_at}</span>
            </div>"##,
            id = t.id,
            content = t.content,
            created_at = t.created_at,
        ));
    }

    if rows.is_empty() {
        rows = r#"<div class="empty-state"><p>Inbox is empty. All thoughts are categorized!</p></div>"#.into();
    }

    let bulk_bar = if !thoughts.is_empty() && !categories.is_empty() {
        format!(
            r#"<form id="bulk-form" method="POST" action="{base}/mindflow/inbox/recategorize"
                  class="form-row" style="margin-bottom:1rem">
                <select name="category_id" required>
                    <option value="" disabled selected>Move to...</option>
                    {cat_options}
                </select>
                <button type="submit" class="btn btn-primary btn-sm">Move selected</button>
            </form>"#
        )
    } else {
        String::new()
    };

    let body = format!(
        r#"<div class="page-header">
            <h1>Inbox</h1>
            <p>{count} uncategorized thoughts</p>
        </div>
        <div class="card">
            <div class="card-body">
                {bulk_bar}
                {rows}
            </div>
        </div>"#,
        count = thoughts.len(),
    );

    Html(render_page(
        "MindFlow — Inbox",
        &mindflow_nav(base, "inbox"),
        &body,
        base,
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

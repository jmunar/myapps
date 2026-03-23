use axum::{
    Extension, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};

use super::mindflow_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/actions", get(list))
        .route("/actions/{id}/toggle", post(toggle))
        .route("/actions/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
struct ActionRow {
    id: i64,
    title: String,
    due_date: Option<String>,
    priority: String,
    status: String,
    thought_id: i64,
    thought_content: String,
    category_name: Option<String>,
    category_color: Option<String>,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let actions: Vec<ActionRow> = sqlx::query_as(
        r#"SELECT a.id, a.title, a.due_date, a.priority, a.status,
                  a.thought_id, t.content AS thought_content,
                  c.name AS category_name, c.color AS category_color
           FROM mindflow_actions a
           JOIN mindflow_thoughts t ON a.thought_id = t.id
           LEFT JOIN mindflow_categories c ON t.category_id = c.id
           WHERE a.user_id = ?
           ORDER BY
               CASE a.status WHEN 'pending' THEN 0 ELSE 1 END,
               CASE a.priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
               a.due_date ASC NULLS LAST"#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut rows = String::new();
    for a in &actions {
        let done_class = if a.status == "done" {
            " action-done"
        } else {
            ""
        };
        let check = if a.status == "done" { "checked" } else { "" };
        let priority_class = match a.priority.as_str() {
            "high" => "priority-high",
            "low" => "priority-low",
            _ => "priority-medium",
        };
        let due = a.due_date.as_deref().unwrap_or("");
        let due_display = if due.is_empty() {
            String::new()
        } else {
            format!(r#"<span class="text-sm text-secondary">{due}</span>"#)
        };

        let cat_badge = match (&a.category_name, &a.category_color) {
            (Some(name), Some(color)) => format!(
                r#"<span class="label-badge label-badge-sm" style="--label-color:{color}">{name}</span>"#
            ),
            _ => String::new(),
        };

        let thought_preview: String = a.thought_content.chars().take(50).collect();

        rows.push_str(&format!(
            r##"<div class="action-item{done_class}">
                <form method="POST" action="{base}/mindflow/actions/{id}/toggle" style="display:inline">
                    <input type="checkbox" {check} onchange="this.form.submit()">
                </form>
                <div style="flex:1">
                    <div class="action-title">{title}</div>
                    <div class="text-sm text-secondary">
                        <a href="{base}/mindflow/thoughts/{thought_id}">{thought_preview}...</a>
                        {cat_badge}
                    </div>
                </div>
                <span class="badge {priority_class}">{priority}</span>
                {due_display}
                <form method="POST" action="{base}/mindflow/actions/{id}/delete" style="display:inline"
                      onsubmit="return confirm('{delete_confirm}')">
                    <button class="btn-icon btn-icon-danger">&times;</button>
                </form>
            </div>"##,
            id = a.id,
            title = a.title,
            priority = a.priority,
            thought_id = a.thought_id,
            delete_confirm = t.act_delete_confirm,
        ));
    }

    if rows.is_empty() {
        rows = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.act_no_actions
        );
    }

    let pending = actions.iter().filter(|a| a.status == "pending").count();
    let done = actions.iter().filter(|a| a.status == "done").count();

    let act_title = t.act_title;

    let body = format!(
        r#"<div class="page-header">
            <h1>{act_title}</h1>
            <p>{pending} pending, {done} done</p>
        </div>
        <div class="card">
            <div class="card-body">
                {rows}
            </div>
        </div>"#
    );

    Html(render_page(
        &format!("MindFlow \u{2014} {}", t.actions),
        &mindflow_nav(base, "actions", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn toggle(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    sqlx::query(
        r#"UPDATE mindflow_actions
           SET status = CASE WHEN status = 'pending' THEN 'done' ELSE 'pending' END,
               completed_at = CASE WHEN status = 'pending' THEN datetime('now') ELSE NULL END
           WHERE id = ? AND user_id = ?"#,
    )
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    Redirect::to(&format!("{base}/mindflow/actions"))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    sqlx::query("DELETE FROM mindflow_actions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();

    Redirect::to(&format!("{base}/mindflow/actions"))
}

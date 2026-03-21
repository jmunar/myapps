use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::mindflow_nav;
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/capture", post(capture))
        .route("/thoughts/{id}", get(detail))
        .route("/thoughts/{id}/comment", post(add_comment))
        .route("/thoughts/{id}/archive", post(archive))
        .route("/thoughts/{id}/recategorize", post(recategorize))
        .route("/thoughts/{id}/action", post(create_action))
        .route("/thoughts/{id}/sub-thought", post(create_sub_thought))
}

// -- Quick capture ────────────────────────────────────────────

#[derive(Deserialize)]
struct CaptureForm {
    content: String,
    category_id: Option<String>,
    parent_thought_id: Option<String>,
}

async fn capture(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Form(form): Form<CaptureForm>,
) -> Html<String> {
    let t = i18n::t(lang);

    let category_id: Option<i64> = form.category_id.as_deref().and_then(|s| s.parse().ok());
    let parent_thought_id: Option<i64> = form
        .parent_thought_id
        .as_deref()
        .and_then(|s| s.parse().ok());

    super::ops::capture_thought(
        &state.pool,
        user_id.0,
        &form.content,
        category_id,
        parent_thought_id,
    )
    .await
    .ok();

    Html(format!(
        r#"<span class="text-sm text-secondary">{}</span>"#,
        t.mf_map_captured
    ))
}

// -- Thought detail ───────────────────────────────────────────

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct ThoughtRow {
    id: i64,
    content: String,
    status: String,
    category_id: Option<i64>,
    category_name: Option<String>,
    category_color: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct CommentRow {
    id: i64,
    content: String,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct ActionRow {
    id: i64,
    title: String,
    due_date: Option<String>,
    priority: String,
    status: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct DescendantThought {
    id: i64,
    parent_thought_id: Option<i64>,
    content: String,
    created_at: String,
    depth: i32,
}

#[derive(sqlx::FromRow)]
struct CategoryOption {
    id: i64,
    name: String,
}

async fn detail(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Result<Html<String>, impl IntoResponse> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let thought: Option<ThoughtRow> = sqlx::query_as(
        r#"SELECT t.id, t.content, t.status, t.category_id,
                  c.name AS category_name, c.color AS category_color,
                  t.created_at, t.updated_at
           FROM mindflow_thoughts t
           LEFT JOIN mindflow_categories c ON t.category_id = c.id
           WHERE t.id = ? AND t.user_id = ?"#,
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(th) = thought else {
        return Err(Redirect::to(&format!("{base}/mindflow")));
    };

    let comments: Vec<CommentRow> = sqlx::query_as(
        "SELECT id, content, created_at FROM mindflow_comments WHERE thought_id = ? ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let actions: Vec<ActionRow> = sqlx::query_as(
        "SELECT id, title, due_date, priority, status FROM mindflow_actions WHERE thought_id = ? ORDER BY status ASC, priority DESC",
    )
    .bind(id)
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

    // All descendants via recursive CTE (full tree under this thought)
    let descendants: Vec<DescendantThought> = sqlx::query_as(
        r#"WITH RECURSIVE tree AS (
               SELECT id, parent_thought_id, content, created_at, 1 AS depth
               FROM mindflow_thoughts
               WHERE parent_thought_id = ? AND user_id = ?
             UNION ALL
               SELECT t.id, t.parent_thought_id, t.content, t.created_at, tree.depth + 1
               FROM mindflow_thoughts t
               JOIN tree ON t.parent_thought_id = tree.id
           )
           SELECT id, parent_thought_id, content, created_at, depth
           FROM tree
           ORDER BY depth ASC, created_at ASC"#,
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Category badge
    let cat_badge = match (&th.category_name, &th.category_color) {
        (Some(name), Some(color)) => {
            format!(r#"<span class="label-badge" style="--label-color:{color}">{name}</span>"#)
        }
        _ => format!(
            r#"<span class="label-badge" style="--label-color:#9E9E9E">{}</span>"#,
            t.mf_thought_inbox_badge
        ),
    };

    let status_badge = if th.status == "archived" {
        format!(
            r#"<span class="badge badge-muted">{}</span>"#,
            t.mf_thought_archived_badge
        )
    } else {
        String::new()
    };

    // Category dropdown
    let mut cat_options = format!(
        r#"<option value="" {}>{}</option>"#,
        if th.category_id.is_none() {
            "selected"
        } else {
            ""
        },
        t.mf_thought_inbox_badge,
    );
    for c in &categories {
        let selected = if th.category_id == Some(c.id) {
            " selected"
        } else {
            ""
        };
        cat_options.push_str(&format!(
            r#"<option value="{id}"{selected}>{name}</option>"#,
            id = c.id,
            name = c.name,
        ));
    }

    // Comments HTML
    let mut comments_html = String::new();
    for c in &comments {
        comments_html.push_str(&format!(
            r#"<div class="comment">
                <div class="comment-content">{content}</div>
                <div class="comment-meta text-sm text-secondary">{created_at}</div>
            </div>"#,
            content = c.content,
            created_at = c.created_at,
        ));
    }

    // Actions HTML
    let mut actions_html = String::new();
    for a in &actions {
        let done_class = if a.status == "done" {
            " action-done"
        } else {
            ""
        };
        let check = if a.status == "done" { "checked" } else { "" };
        let due = a.due_date.as_deref().unwrap_or("");
        let priority_class = match a.priority.as_str() {
            "high" => "priority-high",
            "low" => "priority-low",
            _ => "priority-medium",
        };
        actions_html.push_str(&format!(
            r##"<div class="action-item{done_class}">
                <form method="POST" action="{base}/mindflow/actions/{id}/toggle" style="display:inline">
                    <input type="checkbox" {check} onchange="this.form.submit()">
                </form>
                <span class="action-title">{title}</span>
                <span class="badge {priority_class}">{priority}</span>
                {due_display}
            </div>"##,
            id = a.id,
            title = a.title,
            priority = a.priority,
            due_display = if due.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="text-sm text-secondary">{due}</span>"#)
            },
        ));
    }

    let archive_btn = if th.status == "active" {
        format!(
            r#"<form method="POST" action="{base}/mindflow/thoughts/{id}/archive" style="display:inline">
                <button class="btn btn-secondary btn-sm">{}</button>
            </form>"#,
            t.mf_thought_archive,
        )
    } else {
        format!(
            r#"<form method="POST" action="{base}/mindflow/thoughts/{id}/archive" style="display:inline">
                <button class="btn btn-secondary btn-sm">{}</button>
            </form>"#,
            t.mf_thought_unarchive,
        )
    };

    // Build nested tree HTML from flat descendants list
    let children_html = build_tree_html(&descendants, id, base);

    let thought_title = t.mf_thought_title;
    let move_btn = t.mf_thought_move;
    let comments_heading = t.mf_thought_comments;
    let add_comment_placeholder = t.mf_thought_add_comment;
    let add_btn = t.mf_thought_add_btn;
    let actions_heading = t.mf_thought_actions;
    let new_action_placeholder = t.mf_thought_new_action;
    let low = t.mf_thought_low;
    let medium = t.mf_thought_medium;
    let high = t.mf_thought_high;
    let sub_thoughts_heading = t.mf_thought_sub_thoughts;
    let add_sub_placeholder = t.mf_thought_add_sub;

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>{thought_title}</h1>
                <div>{archive_btn}</div>
            </div>
        </div>

        <div class="card">
            <div class="card-body">
                <div style="margin-bottom:1rem">
                    {cat_badge} {status_badge}
                    <span class="text-sm text-secondary" style="margin-left:0.5rem">{created_at}</span>
                </div>
                <p style="font-size:1.1rem;line-height:1.6">{content}</p>
                <form method="POST" action="{base}/mindflow/thoughts/{id}/recategorize"
                      style="margin-top:1rem" class="form-row">
                    <select name="category_id">{cat_options}</select>
                    <button type="submit" class="btn btn-secondary btn-sm">{move_btn}</button>
                </form>
            </div>
        </div>

        <div class="card mt-2">
            <div class="card-header"><h2>{comments_heading}</h2></div>
            <div class="card-body" id="comments-list">
                {comments_html}
                <form method="POST" action="{base}/mindflow/thoughts/{id}/comment"
                      hx-post="{base}/mindflow/thoughts/{id}/comment"
                      hx-target="#comments-list"
                      hx-swap="innerHTML"
                      class="form-row" style="margin-top:0.5rem">
                    <input type="text" name="content" placeholder="{add_comment_placeholder}" required style="flex:1">
                    <button type="submit" class="btn btn-primary btn-sm">{add_btn}</button>
                </form>
            </div>
        </div>

        <div class="card mt-2">
            <div class="card-header"><h2>{actions_heading}</h2></div>
            <div class="card-body">
                {actions_html}
                <form method="POST" action="{base}/mindflow/thoughts/{id}/action"
                      class="form-row" style="margin-top:0.5rem">
                    <input type="text" name="title" placeholder="{new_action_placeholder}" required style="flex:1">
                    <select name="priority">
                        <option value="low">{low}</option>
                        <option value="medium" selected>{medium}</option>
                        <option value="high">{high}</option>
                    </select>
                    <input type="date" name="due_date">
                    <button type="submit" class="btn btn-primary btn-sm">{add_btn}</button>
                </form>
            </div>
        </div>

        <div class="card mt-2">
            <div class="card-header"><h2>{sub_thoughts_heading}</h2></div>
            <div class="card-body">
                {children_html}
                <form method="POST" action="{base}/mindflow/thoughts/{id}/sub-thought"
                      class="form-row" style="margin-top:0.5rem">
                    <input type="text" name="content" placeholder="{add_sub_placeholder}" required style="flex:1">
                    <button type="submit" class="btn btn-primary btn-sm">{add_btn}</button>
                </form>
            </div>
        </div>"##,
        content = th.content,
        created_at = th.created_at,
    );

    Ok(Html(render_page(
        &format!("MindFlow \u{2014} {}", t.mf_thought_title),
        &mindflow_nav(base, "", lang),
        &body,
        &state.config,
        lang,
    )))
}

// -- Tree rendering helper ────────────────────────────────────

fn build_tree_html(descendants: &[DescendantThought], parent_id: i64, base: &str) -> String {
    let direct_children: Vec<&DescendantThought> = descendants
        .iter()
        .filter(|d| d.parent_thought_id == Some(parent_id))
        .collect();

    if direct_children.is_empty() {
        return String::new();
    }

    let mut html = String::from(r#"<ul class="thought-tree">"#);
    for child in &direct_children {
        let subtree = build_tree_html(descendants, child.id, base);
        html.push_str(&format!(
            r##"<li class="thought-tree-node">
                <a href="{base}/mindflow/thoughts/{id}" class="thought-tree-link">{content}</a>
                <span class="text-sm text-secondary">{created_at}</span>
                {subtree}
            </li>"##,
            id = child.id,
            content = child.content,
            created_at = child.created_at,
        ));
    }
    html.push_str("</ul>");
    html
}

// -- Add comment (returns updated comment list) ──────────────

#[derive(Deserialize)]
struct CommentForm {
    content: String,
}

async fn add_comment(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
    Form(form): Form<CommentForm>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    // Verify ownership
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mindflow_thoughts WHERE id = ? AND user_id = ?)",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if owns {
        sqlx::query("INSERT INTO mindflow_comments (thought_id, content) VALUES (?, ?)")
            .bind(id)
            .bind(&form.content)
            .execute(&state.pool)
            .await
            .ok();
    }

    // Re-render comment list
    let comments: Vec<CommentRow> = sqlx::query_as(
        "SELECT id, content, created_at FROM mindflow_comments WHERE thought_id = ? ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut html = String::new();
    for c in &comments {
        html.push_str(&format!(
            r#"<div class="comment">
                <div class="comment-content">{}</div>
                <div class="comment-meta text-sm text-secondary">{}</div>
            </div>"#,
            c.content, c.created_at,
        ));
    }

    html.push_str(&format!(
        r##"<form method="POST" action="{base}/mindflow/thoughts/{id}/comment"
              hx-post="{base}/mindflow/thoughts/{id}/comment"
              hx-target="#comments-list"
              hx-swap="innerHTML"
              class="form-row" style="margin-top:0.5rem">
            <input type="text" name="content" placeholder="{}" required style="flex:1">
            <button type="submit" class="btn btn-primary btn-sm">{}</button>
        </form>"##,
        t.mf_thought_add_comment, t.mf_thought_add_btn,
    ));

    Html(html)
}

// -- Archive/unarchive thought ───────────────────────────────

async fn archive(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Toggle status
    sqlx::query(
        r#"UPDATE mindflow_thoughts
           SET status = CASE WHEN status = 'active' THEN 'archived' ELSE 'active' END,
               updated_at = datetime('now')
           WHERE id = ? AND user_id = ?"#,
    )
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    Redirect::to(&format!("{base}/mindflow/thoughts/{id}"))
}

// -- Recategorize thought ────────────────────────────────────

#[derive(Deserialize)]
struct RecategorizeForm {
    category_id: Option<String>,
}

async fn recategorize(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<RecategorizeForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let category_id: Option<i64> = form.category_id.as_deref().and_then(|s| s.parse().ok());

    sqlx::query(
        "UPDATE mindflow_thoughts SET category_id = ?, updated_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(category_id)
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    Redirect::to(&format!("{base}/mindflow/thoughts/{id}"))
}

// -- Create action from thought detail ───────────────────────

#[derive(Deserialize)]
struct ActionForm {
    title: String,
    priority: Option<String>,
    due_date: Option<String>,
}

async fn create_action(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(thought_id): Path<i64>,
    Form(form): Form<ActionForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let priority = form.priority.as_deref().unwrap_or("medium");
    let due_date = form.due_date.as_deref().filter(|s| !s.is_empty());

    sqlx::query(
        "INSERT INTO mindflow_actions (thought_id, user_id, title, priority, due_date) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(thought_id)
    .bind(user_id.0)
    .bind(&form.title)
    .bind(priority)
    .bind(due_date)
    .execute(&state.pool)
    .await
    .ok();

    Redirect::to(&format!("{base}/mindflow/thoughts/{thought_id}"))
}

// -- Create sub-thought (nested under parent) ────────────────

#[derive(Deserialize)]
struct SubThoughtForm {
    content: String,
}

async fn create_sub_thought(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(parent_id): Path<i64>,
    Form(form): Form<SubThoughtForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Inherit category from parent thought
    let parent_category: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT category_id FROM mindflow_thoughts WHERE id = ? AND user_id = ?")
            .bind(parent_id)
            .bind(user_id.0)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if let Some((category_id,)) = parent_category {
        super::ops::capture_thought(
            &state.pool,
            user_id.0,
            &form.content,
            category_id,
            Some(parent_id),
        )
        .await
        .ok();
    }

    Redirect::to(&format!("{base}/mindflow/thoughts/{parent_id}"))
}

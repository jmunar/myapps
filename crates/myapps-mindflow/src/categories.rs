use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::mindflow_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/categories", get(list))
        .route("/categories/create", post(create))
        .route("/categories/{id}/edit", post(edit))
        .route("/categories/{id}/archive", post(archive))
        .route("/categories/{id}/unarchive", post(unarchive))
        .route("/categories/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct CategoryRow {
    id: i64,
    name: String,
    color: String,
    icon: Option<String>,
    parent_id: Option<i64>,
    archived: i32,
    thought_count: i32,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let categories: Vec<CategoryRow> = sqlx::query_as(
        r#"SELECT c.id, c.name, c.color, c.icon, c.parent_id, c.archived,
                  (SELECT COUNT(*) FROM mindflow_thoughts
                   WHERE category_id = c.id AND status = 'active') AS thought_count
           FROM mindflow_categories c
           WHERE c.user_id = ?
           ORDER BY c.archived ASC, c.position ASC, c.name ASC"#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let mut items = String::new();
    for c in &categories {
        let icon = c.icon.as_deref().unwrap_or("");
        let archived_class = if c.archived != 0 {
            " category-archived"
        } else {
            ""
        };
        let archived_badge = if c.archived != 0 {
            format!(
                r#"<span class="badge badge-muted">{}</span>"#,
                t.thought_archived_badge
            )
        } else {
            String::new()
        };

        let archive_btn = if c.archived != 0 {
            format!(
                r#"<form method="POST" action="{base}/mindflow/categories/{id}/unarchive" style="display:inline">
                    <button class="btn-icon">{}</button>
                </form>"#,
                t.cat_unarchive,
                id = c.id,
            )
        } else {
            format!(
                r#"<form method="POST" action="{base}/mindflow/categories/{id}/archive" style="display:inline">
                    <button class="btn-icon">{}</button>
                </form>"#,
                t.cat_archive,
                id = c.id,
            )
        };

        let delete_btn = if c.thought_count == 0 {
            format!(
                r#"<form method="POST" action="{base}/mindflow/categories/{id}/delete" style="display:inline"
                     onsubmit="return confirm('{delete_confirm}')">
                    <button class="btn-icon btn-icon-danger">{delete}</button>
                </form>"#,
                id = c.id,
                delete_confirm = t.cat_delete_confirm,
                delete = t.cat_delete,
            )
        } else {
            String::new()
        };

        items.push_str(&format!(
            r##"<div class="label-item{archived_class}" id="category-{id}">
                <div class="label-item-info">
                    <span class="label-badge" style="--label-color:{color}">{icon} {name}</span>
                    <span class="text-secondary text-sm">{count} {thoughts}</span>
                    {archived_badge}
                </div>
                <div class="label-item-actions">
                    <button class="btn-icon"
                            onclick="this.closest('.label-item').querySelector('.label-edit-form').toggleAttribute('hidden')">{edit}</button>
                    {archive_btn}
                    {delete_btn}
                </div>
                <form method="POST" action="{base}/mindflow/categories/{id}/edit" class="label-edit-form" hidden>
                    <input type="text" name="name" value="{name}" required>
                    <input type="color" name="color" value="{color}">
                    <input type="text" name="icon" value="{icon}" placeholder="{icon_placeholder}" style="width:4rem">
                    <button type="submit" class="btn btn-primary btn-sm">{save}</button>
                </form>
            </div>"##,
            id = c.id,
            name = c.name,
            color = c.color,
            count = c.thought_count,
            thoughts = t.cat_thoughts,
            edit = t.cat_edit,
            icon_placeholder = t.cat_icon_placeholder,
            save = t.cat_save,
        ));
    }

    if items.is_empty() {
        items = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.cat_no_categories
        );
    }

    let cat_title = t.cat_title;
    let cat_subtitle = t.cat_subtitle;
    let your_categories = t.cat_your_categories;
    let create_category = t.cat_create;
    let cat_name = t.cat_name;
    let cat_color = t.cat_color;
    let cat_icon = t.cat_icon;
    let cat_icon_placeholder = t.cat_icon_placeholder;
    let cat_create_btn = t.cat_create_btn;

    let body = format!(
        r##"<div class="page-header">
            <h1>{cat_title}</h1>
            <p>{cat_subtitle}</p>
        </div>

        <div class="card" style="max-width:36rem;">
            <div class="card-header"><h2>{your_categories}</h2></div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:36rem;">
            <div class="card-header"><h2>{create_category}</h2></div>
            <div class="card-body">
                <form method="POST" action="{base}/mindflow/categories/create" class="label-create-form">
                    <div class="form-row">
                        <div class="form-group" style="flex:1">
                            <label for="name">{cat_name}</label>
                            <input type="text" id="name" name="name" required placeholder="e.g. Work">
                        </div>
                        <div class="form-group">
                            <label for="color">{cat_color}</label>
                            <input type="color" id="color" name="color" value="#4CAF50">
                        </div>
                        <div class="form-group">
                            <label for="icon">{cat_icon}</label>
                            <input type="text" id="icon" name="icon" placeholder="{cat_icon_placeholder}" style="width:4rem">
                        </div>
                    </div>
                    <button type="submit">{cat_create_btn}</button>
                </form>
            </div>
        </div>"##
    );

    Html(render_page(
        &format!("MindFlow \u{2014} {}", t.categories),
        &mindflow_nav(base, "categories", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct CreateForm {
    name: String,
    color: String,
    icon: Option<String>,
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let icon = form.icon.as_deref().filter(|s| !s.is_empty());
    sqlx::query("INSERT INTO mindflow_categories (user_id, name, color, icon) VALUES (?, ?, ?, ?)")
        .bind(user_id.0)
        .bind(&form.name)
        .bind(&form.color)
        .bind(icon)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/mindflow/categories"))
}

#[derive(Deserialize)]
struct EditForm {
    name: String,
    color: String,
    icon: Option<String>,
}

async fn edit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<EditForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let icon = form.icon.as_deref().filter(|s| !s.is_empty());
    sqlx::query(
        "UPDATE mindflow_categories SET name = ?, color = ?, icon = ? WHERE id = ? AND user_id = ?",
    )
    .bind(&form.name)
    .bind(&form.color)
    .bind(icon)
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();
    Redirect::to(&format!("{base}/mindflow/categories"))
}

async fn archive(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("UPDATE mindflow_categories SET archived = 1 WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/mindflow/categories"))
}

async fn unarchive(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("UPDATE mindflow_categories SET archived = 0 WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/mindflow/categories"))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("DELETE FROM mindflow_categories WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/mindflow/categories"))
}

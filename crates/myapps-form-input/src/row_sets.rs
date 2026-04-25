use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::forms_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/row-sets", get(list))
        .route("/row-sets/create", post(create))
        .route("/row-sets/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct RowSetRow {
    id: i64,
    label: String,
    rows: String,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let row_sets: Vec<RowSetRow> = sqlx::query_as(
        "SELECT id, label, rows FROM form_input_row_sets WHERE user_id = ? ORDER BY label ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let delete_label = t.inp_delete;
    let delete_confirm = t.rs_delete_confirm;
    let rows_count_label = t.rs_rows_count;

    let mut items = String::new();
    for rs in &row_sets {
        let row_count = rs.rows.lines().filter(|l| !l.trim().is_empty()).count();
        let row_preview: String = rs
            .rows
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join(", ");
        let ellipsis = if row_count > 5 { ", …" } else { "" };

        items.push_str(&format!(
            r##"<div class="label-item" id="row-set-{id}">
                <div class="label-item-info">
                    <span class="label-badge" style="--label-color:#1A6B5A">{label}</span>
                    <span class="text-secondary text-sm">{row_count} {rows_count_label}</span>
                    <span class="text-secondary text-sm">{row_preview}{ellipsis}</span>
                </div>
                <div class="label-item-actions">
                    <form method="POST" action="{base}/forms/row-sets/{id}/delete" style="display:inline"
                          onsubmit="return confirm('{delete_confirm}')">
                        <button class="btn-icon btn-icon-danger">{delete_label}</button>
                    </form>
                </div>
            </div>"##,
            id = rs.id,
            label = rs.label,
        ));
    }

    if items.is_empty() {
        items = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.rs_no_row_sets
        );
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="card" style="max-width:40rem;">
            <div class="card-header"><h2>{your_row_sets}</h2></div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:40rem;">
            <div class="card-header"><h2>{add_row_set}</h2></div>
            <div class="card-body">
                <form method="POST" action="{base}/forms/row-sets/create">
                    <div class="form-group">
                        <label for="label">{label_lbl}</label>
                        <input type="text" id="label" name="label" required placeholder="{placeholder}" style="max-width:10rem">
                    </div>
                    <div class="form-group">
                        <label for="rows">{rows_lbl}</label>
                        <textarea id="rows" name="rows" rows="10" required
                                  placeholder="Item one&#10;Item two&#10;Item three"></textarea>
                    </div>
                    <button type="submit">{create_btn}</button>
                </form>
            </div>
        </div>"##,
        title = t.rs_title,
        subtitle = t.rs_subtitle,
        your_row_sets = t.rs_your_row_sets,
        add_row_set = t.rs_add,
        label_lbl = t.rs_label,
        rows_lbl = t.rs_rows,
        placeholder = t.rs_label_hint,
        create_btn = t.rs_create_btn,
    );

    Html(render_page(
        &format!("Forms — {}", t.row_sets),
        &forms_nav(base, "row_sets", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct CreateForm {
    label: String,
    rows: String,
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let cleaned: String = form
        .rows
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    sqlx::query("INSERT INTO form_input_row_sets (user_id, label, rows) VALUES (?, ?, ?)")
        .bind(user_id.0)
        .bind(form.label.trim())
        .bind(&cleaned)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/forms/row-sets"))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    super::ops::delete_row_set(&state.pool, user_id.0, id)
        .await
        .ok();
    Redirect::to(&format!("{base}/forms/row-sets"))
}

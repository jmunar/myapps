use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::classroom_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/classrooms", get(list))
        .route("/classrooms/create", post(create))
        .route("/classrooms/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct ClassroomRow {
    id: i64,
    label: String,
    pupils: String,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let classrooms: Vec<ClassroomRow> = sqlx::query_as(
        "SELECT id, label, pupils FROM classroom_input_classrooms WHERE user_id = ? ORDER BY label ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let delete_label = t.inp_delete;
    let delete_confirm = t.cls_delete_confirm;
    let pupils_count_label = t.cls_pupils_count;

    let mut items = String::new();
    for c in &classrooms {
        let pupil_count = c.pupils.lines().filter(|l| !l.trim().is_empty()).count();
        let pupil_preview: String = c
            .pupils
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join(", ");
        let ellipsis = if pupil_count > 5 { ", …" } else { "" };

        items.push_str(&format!(
            r##"<div class="label-item" id="classroom-{id}">
                <div class="label-item-info">
                    <span class="label-badge" style="--label-color:#1A6B5A">{label}</span>
                    <span class="text-secondary text-sm">{pupil_count} {pupils_count_label}</span>
                    <span class="text-secondary text-sm">{pupil_preview}{ellipsis}</span>
                </div>
                <div class="label-item-actions">
                    <form method="POST" action="{base}/classroom/classrooms/{id}/delete" style="display:inline"
                          onsubmit="return confirm('{delete_confirm}')">
                        <button class="btn-icon btn-icon-danger">{delete_label}</button>
                    </form>
                </div>
            </div>"##,
            id = c.id,
            label = c.label,
        ));
    }

    if items.is_empty() {
        items = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.cls_no_classrooms
        );
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="card" style="max-width:40rem;">
            <div class="card-header"><h2>{your_classrooms}</h2></div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:40rem;">
            <div class="card-header"><h2>{add_classroom}</h2></div>
            <div class="card-body">
                <form method="POST" action="{base}/classroom/classrooms/create">
                    <div class="form-group">
                        <label for="label">{label_lbl}</label>
                        <input type="text" id="label" name="label" required placeholder="{placeholder}" style="max-width:10rem">
                    </div>
                    <div class="form-group">
                        <label for="pupils">{pupils_lbl}</label>
                        <textarea id="pupils" name="pupils" rows="10" required
                                  placeholder="María García&#10;Pedro López&#10;Ana Martínez"></textarea>
                    </div>
                    <button type="submit">{create_btn}</button>
                </form>
            </div>
        </div>"##,
        title = t.cls_title,
        subtitle = t.cls_subtitle,
        your_classrooms = t.cls_your_classrooms,
        add_classroom = t.cls_add,
        label_lbl = t.cls_label,
        pupils_lbl = t.cls_pupils,
        placeholder = t.cls_pupils_hint,
        create_btn = t.cls_create_btn,
    );

    Html(render_page(
        &format!("Classroom — {}", t.classrooms),
        &classroom_nav(base, "classrooms", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct CreateForm {
    label: String,
    pupils: String,
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    // Clean up pupils: remove empty lines, trim whitespace
    let cleaned: String = form
        .pupils
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    sqlx::query("INSERT INTO classroom_input_classrooms (user_id, label, pupils) VALUES (?, ?, ?)")
        .bind(user_id.0)
        .bind(form.label.trim())
        .bind(&cleaned)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom/classrooms"))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    super::ops::delete_classroom(&state.pool, user_id.0, id)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom/classrooms"))
}

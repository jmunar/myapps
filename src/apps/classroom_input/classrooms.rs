use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::classroom_nav;
use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;

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
) -> Html<String> {
    let base = &state.config.base_path;

    let classrooms: Vec<ClassroomRow> = sqlx::query_as(
        "SELECT id, label, pupils FROM classroom_classrooms WHERE user_id = ? ORDER BY label ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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
                    <span class="text-secondary text-sm">{pupil_count} pupils</span>
                    <span class="text-secondary text-sm">{pupil_preview}{ellipsis}</span>
                </div>
                <div class="label-item-actions">
                    <form method="POST" action="{base}/classroom/classrooms/{id}/delete" style="display:inline"
                          onsubmit="return confirm('Delete this classroom and all its inputs?')">
                        <button class="btn-icon btn-icon-danger">Delete</button>
                    </form>
                </div>
            </div>"##,
            id = c.id,
            label = c.label,
        ));
    }

    if items.is_empty() {
        items =
            r#"<div class="empty-state"><p>No classrooms yet. Create one below.</p></div>"#.into();
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>Classrooms</h1>
            <p>Manage your classrooms and their pupil lists</p>
        </div>

        <div class="card" style="max-width:40rem;">
            <div class="card-header"><h2>Your classrooms</h2></div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:40rem;">
            <div class="card-header"><h2>Add classroom</h2></div>
            <div class="card-body">
                <form method="POST" action="{base}/classroom/classrooms/create">
                    <div class="form-group">
                        <label for="label">Label</label>
                        <input type="text" id="label" name="label" required placeholder="e.g. 1-A" style="max-width:10rem">
                    </div>
                    <div class="form-group">
                        <label for="pupils">Pupils (one per line — paste from clipboard)</label>
                        <textarea id="pupils" name="pupils" rows="10" required
                                  placeholder="María García&#10;Pedro López&#10;Ana Martínez"></textarea>
                    </div>
                    <button type="submit">Create classroom</button>
                </form>
            </div>
        </div>"##
    );

    Html(render_page(
        "Classroom — Classrooms",
        &classroom_nav(base, "classrooms"),
        &body,
        base,
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

    sqlx::query("INSERT INTO classroom_classrooms (user_id, label, pupils) VALUES (?, ?, ?)")
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
    // Delete associated inputs first
    sqlx::query("DELETE FROM classroom_inputs WHERE classroom_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM classroom_classrooms WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom/classrooms"))
}

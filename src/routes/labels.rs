use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::AppState;
use crate::auth::UserId;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels))
        .route("/labels/create", post(create_label))
        .route("/labels/{id}/delete", post(delete_label))
        .route("/labels/{id}/edit", post(edit_label))
}

// ── List labels ──────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct LabelRow {
    id: i64,
    name: String,
    color: Option<String>,
    rule_count: i32,
    txn_count: i32,
}

async fn list_labels(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

    let labels: Vec<LabelRow> = sqlx::query_as(
        r#"SELECT l.id, l.name, l.color,
                  (SELECT COUNT(*) FROM label_rules WHERE label_id = l.id) AS rule_count,
                  (SELECT COUNT(*) FROM allocations WHERE label_id = l.id) AS txn_count
           FROM labels l
           WHERE l.user_id = ?
           ORDER BY l.name"#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut items = String::new();
    for l in &labels {
        let color = l.color.as_deref().unwrap_or("#6B6B6B");
        items.push_str(&format!(
            r#"<div class="label-item" id="label-{id}">
                <div class="label-item-info">
                    <span class="label-badge" style="--label-color:{color}">{name}</span>
                    <span class="text-secondary text-sm">{rules}r / {txns}t</span>
                </div>
                <div class="label-item-actions">
                    <button class="btn-icon" onclick="this.closest('.label-item').querySelector('.label-edit-form').toggleAttribute('hidden')">Edit</button>
                    <form method="POST" action="{base}/labels/{id}/delete" style="display:inline"
                          onsubmit="return confirm('Delete label \'{name}\'?')">
                        <button class="btn-icon btn-icon-danger">Delete</button>
                    </form>
                </div>
                <form method="POST" action="{base}/labels/{id}/edit" class="label-edit-form" hidden>
                    <input type="text" name="name" value="{name}" required>
                    <input type="color" name="color" value="{color}">
                    <button type="submit" class="btn btn-primary btn-sm">Save</button>
                </form>
            </div>"#,
            id = l.id,
            name = l.name,
            color = color,
            rules = l.rule_count,
            txns = l.txn_count,
        ));
    }

    if items.is_empty() {
        items = r#"<div class="empty-state"><p>No labels yet. Create one below.</p></div>"#.into();
    }

    let default_color = "#4CAF50";
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LeanFin — Labels</title>
    <link rel="stylesheet" href="{base}/static/style.css">
</head>
<body>
    <nav>
        <span class="brand">LeanFin</span>
        <a href="{base}/">Transactions</a>
        <a href="{base}/accounts">Accounts</a>
        <a href="{base}/labels" class="active">Labels</a>
        <a href="{base}/logout" class="nav-right">Log out</a>
    </nav>
    <main>
        <div class="page-header">
            <h1>Labels</h1>
            <p>Organize your transactions with labels</p>
        </div>

        <div class="card" style="max-width:36rem;">
            <div class="card-header">
                <h2>Your labels</h2>
            </div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:36rem;">
            <div class="card-header">
                <h2>Create label</h2>
            </div>
            <div class="card-body">
                <form method="POST" action="{base}/labels/create" class="label-create-form">
                    <div class="form-row">
                        <div class="form-group" style="flex:1">
                            <label for="name">Name</label>
                            <input type="text" id="name" name="name" required placeholder="e.g. Groceries">
                        </div>
                        <div class="form-group">
                            <label for="color">Color</label>
                            <input type="color" id="color" name="color" value="{default_color}">
                        </div>
                    </div>
                    <button type="submit">Create label</button>
                </form>
            </div>
        </div>
    </main>
</body>
</html>"#
    ))
}

// ── Create label ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateLabelForm {
    name: String,
    color: String,
}

async fn create_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateLabelForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    if let Err(e) = sqlx::query("INSERT INTO labels (user_id, name, color) VALUES (?, ?, ?)")
        .bind(user_id.0)
        .bind(&form.name)
        .bind(&form.color)
        .execute(&state.pool)
        .await
    {
        tracing::error!("Failed to create label: {e}");
    }
    Redirect::to(&format!("{base}/labels"))
}

// ── Edit label ───────────────────────────────────────────────

#[derive(Deserialize)]
struct EditLabelForm {
    name: String,
    color: String,
}

async fn edit_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<EditLabelForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("UPDATE labels SET name = ?, color = ? WHERE id = ? AND user_id = ?")
        .bind(&form.name)
        .bind(&form.color)
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/labels"))
}

// ── Delete label ─────────────────────────────────────────────

async fn delete_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("DELETE FROM labels WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/labels"))
}

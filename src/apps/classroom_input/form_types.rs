use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use super::classroom_nav;
use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/form-types", get(list))
        .route("/form-types/create", post(create))
        .route("/form-types/{id}/edit", get(edit_page).post(edit))
        .route("/form-types/{id}/delete", post(delete))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ColumnDef {
    pub name: String,
    /// "text", "number", or "bool"
    #[serde(rename = "type")]
    pub col_type: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct FormTypeRow {
    id: i64,
    name: String,
    columns_json: String,
}

fn parse_columns(json: &str) -> Vec<ColumnDef> {
    serde_json::from_str(json).unwrap_or_default()
}

fn render_column_list(cols: &[ColumnDef]) -> String {
    if cols.is_empty() {
        return String::from("<em>No columns</em>");
    }
    let mut out = String::new();
    for c in cols {
        let type_label = match c.col_type.as_str() {
            "number" => "Number",
            "bool" => "Yes/No",
            _ => "Text",
        };
        out.push_str(&format!(
            r#"<span class="label-badge" style="--label-color:#3182CE">{name} <small>({type_label})</small></span> "#,
            name = c.name,
        ));
    }
    out
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

    let form_types: Vec<FormTypeRow> = sqlx::query_as(
        "SELECT id, name, columns_json FROM classroom_form_types WHERE user_id = ? ORDER BY name ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut items = String::new();
    for ft in &form_types {
        let cols = parse_columns(&ft.columns_json);
        let col_html = render_column_list(&cols);

        items.push_str(&format!(
            r##"<div class="label-item" id="formtype-{id}">
                <div class="label-item-info" style="flex-direction:column;align-items:flex-start;gap:0.25rem">
                    <strong>{name}</strong>
                    <div>{col_html}</div>
                </div>
                <div class="label-item-actions">
                    <a href="{base}/classroom/form-types/{id}/edit" class="btn-icon">Edit</a>
                    <form method="POST" action="{base}/classroom/form-types/{id}/delete" style="display:inline"
                          onsubmit="return confirm('Delete this form type?')">
                        <button class="btn-icon btn-icon-danger">Delete</button>
                    </form>
                </div>
            </div>"##,
            id = ft.id,
            name = ft.name,
        ));
    }

    if items.is_empty() {
        items =
            r#"<div class="empty-state"><p>No form types yet. Create one below.</p></div>"#.into();
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>Form Types</h1>
            <p>Define column layouts for your input forms</p>
        </div>

        <div class="card" style="max-width:40rem;">
            <div class="card-header"><h2>Your form types</h2></div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:40rem;">
            <div class="card-header"><h2>Create form type</h2></div>
            <div class="card-body">
                <form method="POST" action="{base}/classroom/form-types/create">
                    <div class="form-group">
                        <label for="name">Name</label>
                        <input type="text" id="name" name="name" required placeholder="e.g. Weekly quiz">
                    </div>
                    <div class="form-group">
                        <label>Columns</label>
                        <div id="columns-editor" class="ci-columns-editor">
                            <div class="ci-column-row">
                                <input type="text" name="col_name[]" placeholder="Column name" required>
                                <select name="col_type[]">
                                    <option value="text">Text</option>
                                    <option value="number">Number</option>
                                    <option value="bool">Yes / No</option>
                                </select>
                                <button type="button" class="btn-icon btn-icon-danger" onclick="this.closest('.ci-column-row').remove()">×</button>
                            </div>
                        </div>
                        <button type="button" class="btn btn-secondary btn-sm mt-1"
                                onclick="addColumnRow(document.getElementById('columns-editor'))">+ Add column</button>
                    </div>
                    <button type="submit" class="mt-1">Create form type</button>
                </form>
            </div>
        </div>

        <script>
        function addColumnRow(container) {{
            var row = document.createElement('div');
            row.className = 'ci-column-row';
            row.innerHTML = '<input type="text" name="col_name[]" placeholder="Column name" required>'
                + '<select name="col_type[]"><option value="text">Text</option><option value="number">Number</option><option value="bool">Yes / No</option></select>'
                + '<button type="button" class="btn-icon btn-icon-danger" onclick="this.closest(\'.ci-column-row\').remove()">×</button>';
            container.appendChild(row);
        }}
        </script>"##
    );

    Html(render_page(
        "Classroom — Form Types",
        &classroom_nav(base, "form_types"),
        &body,
        base,
    ))
}

#[derive(Deserialize)]
struct CreateForm {
    name: String,
    #[serde(rename = "col_name[]")]
    col_name: Vec<String>,
    #[serde(rename = "col_type[]")]
    col_type: Vec<String>,
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let columns: Vec<ColumnDef> = form
        .col_name
        .iter()
        .zip(form.col_type.iter())
        .filter(|(n, _)| !n.trim().is_empty())
        .map(|(n, t)| ColumnDef {
            name: n.trim().to_string(),
            col_type: t.clone(),
        })
        .collect();
    let json = serde_json::to_string(&columns).unwrap_or_default();

    sqlx::query("INSERT INTO classroom_form_types (user_id, name, columns_json) VALUES (?, ?, ?)")
        .bind(user_id.0)
        .bind(form.name.trim())
        .bind(&json)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom/form-types"))
}

async fn edit_page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

    let ft: Option<FormTypeRow> = sqlx::query_as(
        "SELECT id, name, columns_json FROM classroom_form_types WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(ft) = ft else {
        return Html(render_page(
            "Classroom — Not Found",
            &classroom_nav(base, "form_types"),
            r#"<div class="empty-state"><p>Form type not found.</p></div>"#,
            base,
        ));
    };

    let cols = parse_columns(&ft.columns_json);
    let mut col_rows = String::new();
    for c in &cols {
        let sel_text = if c.col_type == "text" {
            " selected"
        } else {
            ""
        };
        let sel_num = if c.col_type == "number" {
            " selected"
        } else {
            ""
        };
        let sel_bool = if c.col_type == "bool" {
            " selected"
        } else {
            ""
        };
        col_rows.push_str(&format!(
            r#"<div class="ci-column-row">
                <input type="text" name="col_name[]" value="{name}" required>
                <select name="col_type[]">
                    <option value="text"{sel_text}>Text</option>
                    <option value="number"{sel_num}>Number</option>
                    <option value="bool"{sel_bool}>Yes / No</option>
                </select>
                <button type="button" class="btn-icon btn-icon-danger" onclick="this.closest('.ci-column-row').remove()">×</button>
            </div>"#,
            name = c.name,
        ));
    }

    if col_rows.is_empty() {
        col_rows = r#"<div class="ci-column-row">
            <input type="text" name="col_name[]" placeholder="Column name" required>
            <select name="col_type[]"><option value="text">Text</option><option value="number">Number</option><option value="bool">Yes / No</option></select>
            <button type="button" class="btn-icon btn-icon-danger" onclick="this.closest('.ci-column-row').remove()">×</button>
        </div>"#
            .into();
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>Edit Form Type</h1>
        </div>

        <div class="card" style="max-width:40rem;">
            <div class="card-body">
                <form method="POST" action="{base}/classroom/form-types/{id}/edit">
                    <div class="form-group">
                        <label for="name">Name</label>
                        <input type="text" id="name" name="name" value="{name}" required>
                    </div>
                    <div class="form-group">
                        <label>Columns</label>
                        <div id="columns-editor" class="ci-columns-editor">
                            {col_rows}
                        </div>
                        <button type="button" class="btn btn-secondary btn-sm mt-1"
                                onclick="addColumnRow(document.getElementById('columns-editor'))">+ Add column</button>
                    </div>
                    <div style="display:flex;gap:0.5rem;margin-top:0.75rem">
                        <button type="submit" class="btn btn-primary">Save changes</button>
                        <a href="{base}/classroom/form-types" class="btn btn-secondary">Cancel</a>
                    </div>
                </form>
            </div>
        </div>

        <script>
        function addColumnRow(container) {{
            var row = document.createElement('div');
            row.className = 'ci-column-row';
            row.innerHTML = '<input type="text" name="col_name[]" placeholder="Column name" required>'
                + '<select name="col_type[]"><option value="text">Text</option><option value="number">Number</option><option value="bool">Yes / No</option></select>'
                + '<button type="button" class="btn-icon btn-icon-danger" onclick="this.closest(\'.ci-column-row\').remove()">×</button>';
            container.appendChild(row);
        }}
        </script>"##,
        id = ft.id,
        name = ft.name,
    );

    Html(render_page(
        "Classroom — Edit Form Type",
        &classroom_nav(base, "form_types"),
        &body,
        base,
    ))
}

#[derive(Deserialize)]
struct EditForm {
    name: String,
    #[serde(rename = "col_name[]")]
    col_name: Vec<String>,
    #[serde(rename = "col_type[]")]
    col_type: Vec<String>,
}

async fn edit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<EditForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let columns: Vec<ColumnDef> = form
        .col_name
        .iter()
        .zip(form.col_type.iter())
        .filter(|(n, _)| !n.trim().is_empty())
        .map(|(n, t)| ColumnDef {
            name: n.trim().to_string(),
            col_type: t.clone(),
        })
        .collect();
    let json = serde_json::to_string(&columns).unwrap_or_default();

    sqlx::query(
        "UPDATE classroom_form_types SET name = ?, columns_json = ?, updated_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(form.name.trim())
    .bind(&json)
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();
    Redirect::to(&format!("{base}/classroom/form-types"))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    // Delete associated inputs first
    sqlx::query("DELETE FROM classroom_inputs WHERE form_type_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM classroom_form_types WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom/form-types"))
}

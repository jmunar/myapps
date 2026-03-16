use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::classroom_nav;
use super::form_types::ColumnDef;
use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/new", get(new_input_page))
        .route("/inputs/create", post(create))
        .route("/inputs/{id}", get(view))
        .route("/inputs/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct InputRow {
    id: i64,
    classroom_id: i64,
    form_type_id: i64,
    name: String,
    csv_data: String,
    created_at: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct ClassroomRow {
    id: i64,
    label: String,
    pupils: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct FormTypeRow {
    id: i64,
    name: String,
    columns_json: String,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

    let inputs: Vec<InputRow> = sqlx::query_as(
        "SELECT id, classroom_id, form_type_id, name, csv_data, created_at
         FROM classroom_inputs WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Pre-fetch classrooms and form types for labels
    let classrooms: Vec<ClassroomRow> =
        sqlx::query_as("SELECT id, label, pupils FROM classroom_classrooms WHERE user_id = ?")
            .bind(user_id.0)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    let form_types: Vec<FormTypeRow> =
        sqlx::query_as("SELECT id, name, columns_json FROM classroom_form_types WHERE user_id = ?")
            .bind(user_id.0)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    let mut rows_html = String::new();
    for inp in &inputs {
        let cls_label = classrooms
            .iter()
            .find(|c| c.id == inp.classroom_id)
            .map(|c| c.label.as_str())
            .unwrap_or("?");
        let ft_name = form_types
            .iter()
            .find(|f| f.id == inp.form_type_id)
            .map(|f| f.name.as_str())
            .unwrap_or("?");
        let row_count = inp.csv_data.lines().count().saturating_sub(1); // minus header
        let date = &inp.created_at[..10.min(inp.created_at.len())];

        rows_html.push_str(&format!(
            r##"<tr>
                <td><a href="{base}/classroom/inputs/{id}">{name}</a></td>
                <td><span class="label-badge" style="--label-color:#1A6B5A">{cls_label}</span></td>
                <td>{ft_name}</td>
                <td class="mono">{row_count}</td>
                <td class="txn-date">{date}</td>
                <td>
                    <form method="POST" action="{base}/classroom/inputs/{id}/delete" style="display:inline"
                          onsubmit="return confirm('Delete this input?')">
                        <button class="btn-icon btn-icon-danger">Delete</button>
                    </form>
                </td>
            </tr>"##,
            id = inp.id,
            name = inp.name,
        ));
    }

    let table_or_empty = if rows_html.is_empty() {
        r#"<div class="empty-state"><p>No inputs yet. Create one to get started.</p></div>"#
            .to_string()
    } else {
        format!(
            r#"<table>
                <thead><tr>
                    <th>Name</th><th>Classroom</th><th>Form Type</th><th>Rows</th><th>Date</th><th></th>
                </tr></thead>
                <tbody>{rows_html}</tbody>
            </table>"#
        )
    };

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <div>
                    <h1>Inputs</h1>
                    <p>Classroom data inputs</p>
                </div>
                <a href="{base}/classroom/new" class="btn btn-primary">+ New input</a>
            </div>
        </div>

        <div class="card">
            {table_or_empty}
        </div>"##
    );

    Html(render_page(
        "Classroom — Inputs",
        &classroom_nav(base, "inputs"),
        &body,
        base,
    ))
}

async fn new_input_page(
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

    let form_types: Vec<FormTypeRow> = sqlx::query_as(
        "SELECT id, name, columns_json FROM classroom_form_types WHERE user_id = ? ORDER BY name ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if classrooms.is_empty() || form_types.is_empty() {
        let msg = if classrooms.is_empty() && form_types.is_empty() {
            "You need to create at least one <a href=\"{base}/classroom/classrooms\">classroom</a> and one <a href=\"{base}/classroom/form-types\">form type</a> first."
        } else if classrooms.is_empty() {
            "You need to create at least one <a href=\"{base}/classroom/classrooms\">classroom</a> first."
        } else {
            "You need to create at least one <a href=\"{base}/classroom/form-types\">form type</a> first."
        };
        let msg = msg.replace("{base}", base);
        let body = format!(
            r#"<div class="page-header"><h1>New Input</h1></div>
            <div class="card" style="max-width:36rem"><div class="card-body"><p>{msg}</p></div></div>"#
        );
        return Html(render_page(
            "Classroom — New Input",
            &classroom_nav(base, "inputs"),
            &body,
            base,
        ));
    }

    // Serialize classrooms and form types as JSON for the JS grid builder
    let classrooms_json: Vec<serde_json::Value> = classrooms
        .iter()
        .map(|c| {
            let pupils: Vec<&str> = c.pupils.lines().filter(|l| !l.trim().is_empty()).collect();
            serde_json::json!({"id": c.id, "label": c.label, "pupils": pupils})
        })
        .collect();
    let form_types_json: Vec<serde_json::Value> = form_types
        .iter()
        .map(|f| {
            let cols: Vec<ColumnDef> = serde_json::from_str(&f.columns_json).unwrap_or_default();
            serde_json::json!({"id": f.id, "name": f.name, "columns": cols})
        })
        .collect();

    let cls_json = serde_json::to_string(&classrooms_json).unwrap_or_default();
    let ft_json = serde_json::to_string(&form_types_json).unwrap_or_default();

    // Classroom select options
    let mut cls_opts = String::new();
    for c in &classrooms {
        cls_opts.push_str(&format!(r#"<option value="{}">{}</option>"#, c.id, c.label));
    }

    // Form type select options
    let mut ft_opts = String::new();
    for f in &form_types {
        ft_opts.push_str(&format!(r#"<option value="{}">{}</option>"#, f.id, f.name));
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>New Input</h1>
            <p>Select classroom and form type, then fill in the grid</p>
        </div>

        <div class="card" style="max-width:60rem;">
            <div class="card-body">
                <form method="POST" action="{base}/classroom/inputs/create" id="input-form">
                    <div class="form-row" style="align-items:flex-end;gap:1rem;flex-wrap:wrap">
                        <div class="form-group">
                            <label for="classroom_id">Classroom</label>
                            <select id="classroom_id" name="classroom_id" required>{cls_opts}</select>
                        </div>
                        <div class="form-group">
                            <label for="form_type_id">Form Type</label>
                            <select id="form_type_id" name="form_type_id" required>{ft_opts}</select>
                        </div>
                        <div class="form-group" style="flex:1">
                            <label for="input_name">Input Name</label>
                            <input type="text" id="input_name" name="name" required placeholder="e.g. Week 12 quiz">
                        </div>
                    </div>

                    <div id="grid-container" class="ci-grid-container mt-2"></div>

                    <input type="hidden" name="csv_data" id="csv_data">
                    <button type="submit" class="btn btn-primary mt-2" id="submit-btn">Save input</button>
                </form>
            </div>
        </div>

        <script>
        (function() {{
            var classrooms = {cls_json};
            var formTypes = {ft_json};

            var clsSel = document.getElementById('classroom_id');
            var ftSel = document.getElementById('form_type_id');
            var gridContainer = document.getElementById('grid-container');
            var csvInput = document.getElementById('csv_data');
            var form = document.getElementById('input-form');

            function buildGrid() {{
                var clsId = parseInt(clsSel.value);
                var ftId = parseInt(ftSel.value);
                var cls = classrooms.find(function(c) {{ return c.id === clsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                if (!cls || !ft || ft.columns.length === 0) {{
                    gridContainer.innerHTML = '<p class="text-secondary">Select a classroom and form type with columns.</p>';
                    return;
                }}

                var pupils = cls.pupils;
                var cols = ft.columns;

                var html = '<table class="ci-input-table"><thead><tr><th class="ci-th-pupil">Pupil</th>';
                for (var i = 0; i < cols.length; i++) {{
                    html += '<th>' + cols[i].name + '</th>';
                }}
                html += '</tr></thead><tbody>';

                for (var r = 0; r < pupils.length; r++) {{
                    html += '<tr><td class="ci-pupil-name">' + pupils[r] + '</td>';
                    for (var c = 0; c < cols.length; c++) {{
                        var colType = cols[c].type || cols[c].col_type || 'text';
                        if (colType === 'bool') {{
                            html += '<td class="ci-col-bool"><select data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-select">'
                                + '<option value=""></option><option value="Yes">Yes</option><option value="No">No</option></select></td>';
                        }} else if (colType === 'number') {{
                            html += '<td class="ci-col-number"><input type="number" step="any" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input" inputmode="decimal"></td>';
                        }} else {{
                            html += '<td><input type="text" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input"></td>';
                        }}
                    }}
                    html += '</tr>';
                }}
                html += '</tbody></table>';
                gridContainer.innerHTML = html;
                setupNav();
            }}

            function setupNav() {{
                var clsId = parseInt(clsSel.value);
                var ftId = parseInt(ftSel.value);
                var cls = classrooms.find(function(c) {{ return c.id === clsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                var maxR = cls ? cls.pupils.length - 1 : 0;
                var maxC = ft ? ft.columns.length - 1 : 0;

                var cells = gridContainer.querySelectorAll('.ci-cell');
                cells.forEach(function(cell) {{
                    cell.addEventListener('keydown', function(e) {{
                        var r = parseInt(this.dataset.r);
                        var c = parseInt(this.dataset.c);
                        var nextR = r, nextC = c;
                        var atEnd = this.tagName === 'SELECT' || this.selectionStart == null || this.selectionStart === this.value.length;
                        var atStart = this.tagName === 'SELECT' || this.selectionStart == null || this.selectionStart === 0;

                        if (e.key === 'ArrowDown' || e.key === 'Enter') {{
                            e.preventDefault();
                            nextR = r + 1;
                        }} else if (e.key === 'ArrowUp') {{
                            e.preventDefault();
                            nextR = r - 1;
                        }} else if (e.key === 'ArrowRight' && atEnd) {{
                            e.preventDefault();
                            if (c < maxC) {{
                                nextC = c + 1;
                            }} else if (r < maxR) {{
                                nextR = r + 1;
                                nextC = 0;
                            }}
                        }} else if (e.key === 'ArrowLeft' && atStart) {{
                            e.preventDefault();
                            if (c > 0) {{
                                nextC = c - 1;
                            }} else if (r > 0) {{
                                nextR = r - 1;
                                nextC = maxC;
                            }}
                        }} else {{
                            return;
                        }}

                        var target = gridContainer.querySelector('[data-r="' + nextR + '"][data-c="' + nextC + '"]');
                        if (target) target.focus();
                    }});
                }});
            }}

            clsSel.addEventListener('change', buildGrid);
            ftSel.addEventListener('change', buildGrid);
            buildGrid();

            form.addEventListener('submit', function() {{
                var clsId = parseInt(clsSel.value);
                var ftId = parseInt(ftSel.value);
                var cls = classrooms.find(function(c) {{ return c.id === clsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                if (!cls || !ft) return;

                var pupils = cls.pupils;
                var cols = ft.columns;

                var lines = [];
                var header = ['Pupil'];
                for (var i = 0; i < cols.length; i++) header.push(csvEscape(cols[i].name));
                lines.push(header.join(','));

                for (var r = 0; r < pupils.length; r++) {{
                    var row = [csvEscape(pupils[r])];
                    for (var c = 0; c < cols.length; c++) {{
                        var cell = gridContainer.querySelector('[data-r="' + r + '"][data-c="' + c + '"]');
                        row.push(csvEscape(cell ? cell.value : ''));
                    }}
                    lines.push(row.join(','));
                }}
                csvInput.value = lines.join('\n');
            }});

            function csvEscape(val) {{
                if (!val) return '';
                val = String(val);
                if (val.indexOf(',') >= 0 || val.indexOf('"') >= 0 || val.indexOf('\n') >= 0) {{
                    return '"' + val.replace(/"/g, '""') + '"';
                }}
                return val;
            }}
        }})();
        </script>"##
    );

    Html(render_page(
        "Classroom — New Input",
        &classroom_nav(base, "inputs"),
        &body,
        base,
    ))
}

#[derive(Deserialize)]
struct CreateInputForm {
    classroom_id: i64,
    form_type_id: i64,
    name: String,
    csv_data: String,
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateInputForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query(
        "INSERT INTO classroom_inputs (user_id, classroom_id, form_type_id, name, csv_data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id.0)
    .bind(form.classroom_id)
    .bind(form.form_type_id)
    .bind(form.name.trim())
    .bind(&form.csv_data)
    .execute(&state.pool)
    .await
    .ok();
    Redirect::to(&format!("{base}/classroom"))
}

async fn view(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

    let inp: Option<InputRow> = sqlx::query_as(
        "SELECT id, classroom_id, form_type_id, name, csv_data, created_at
         FROM classroom_inputs WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(inp) = inp else {
        return Html(render_page(
            "Classroom — Not Found",
            &classroom_nav(base, "inputs"),
            r#"<div class="empty-state"><p>Input not found.</p></div>"#,
            base,
        ));
    };

    // Parse CSV into HTML table
    let lines: Vec<&str> = inp.csv_data.lines().collect();
    let mut table_html = String::from("<table><thead><tr>");
    if let Some(header) = lines.first() {
        for col in parse_csv_line(header) {
            table_html.push_str(&format!("<th>{col}</th>"));
        }
    }
    table_html.push_str("</tr></thead><tbody>");
    for line in lines.iter().skip(1) {
        table_html.push_str("<tr>");
        let fields = parse_csv_line(line);
        for (i, field) in fields.iter().enumerate() {
            if i == 0 {
                table_html.push_str(&format!(r#"<td class="ci-pupil-name">{field}</td>"#));
            } else {
                table_html.push_str(&format!("<td>{field}</td>"));
            }
        }
        table_html.push_str("</tr>");
    }
    table_html.push_str("</tbody></table>");

    let cls_label: Option<String> =
        sqlx::query_scalar("SELECT label FROM classroom_classrooms WHERE id = ?")
            .bind(inp.classroom_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let ft_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM classroom_form_types WHERE id = ?")
            .bind(inp.form_type_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let date = &inp.created_at[..10.min(inp.created_at.len())];

    let body = format!(
        r##"<div class="page-header">
            <h1>{name}</h1>
            <p>
                <span class="label-badge" style="--label-color:#1A6B5A">{cls_label}</span>
                {ft_name} — {date}
            </p>
        </div>

        <div class="card">
            {table_html}
        </div>

        <div class="mt-2">
            <a href="{base}/classroom" class="btn btn-secondary">Back to inputs</a>
        </div>"##,
        name = inp.name,
        cls_label = cls_label.as_deref().unwrap_or("?"),
        ft_name = ft_name.as_deref().unwrap_or("?"),
    );

    Html(render_page(
        &format!("Classroom — {}", inp.name),
        &classroom_nav(base, "inputs"),
        &body,
        base,
    ))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("DELETE FROM classroom_inputs WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/classroom"))
}

/// Simple CSV line parser that handles quoted fields.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

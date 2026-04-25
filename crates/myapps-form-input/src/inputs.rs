use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::form_types::ColumnDef;
use super::forms_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

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
    row_set_id: Option<i64>,
    form_type_id: i64,
    name: String,
    csv_data: String,
    created_at: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct RowSetRow {
    id: i64,
    label: String,
    rows: String,
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
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let inputs: Vec<InputRow> = sqlx::query_as(
        "SELECT id, row_set_id, form_type_id, name, csv_data, created_at
         FROM form_input_inputs WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let row_sets: Vec<RowSetRow> =
        sqlx::query_as("SELECT id, label, rows FROM form_input_row_sets WHERE user_id = ?")
            .bind(user_id.0)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            });

    let form_types: Vec<FormTypeRow> = sqlx::query_as(
        "SELECT id, name, columns_json FROM form_input_form_types WHERE user_id = ?",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let delete_label = t.inp_delete;
    let delete_confirm = t.inp_delete_confirm;

    let mut rows_html = String::new();
    for inp in &inputs {
        let rs_label = inp
            .row_set_id
            .and_then(|rsid| {
                row_sets
                    .iter()
                    .find(|rs| rs.id == rsid)
                    .map(|rs| rs.label.as_str())
            })
            .unwrap_or("—");
        let ft_name = form_types
            .iter()
            .find(|f| f.id == inp.form_type_id)
            .map(|f| f.name.as_str())
            .unwrap_or("?");
        let row_count = inp.csv_data.lines().count().saturating_sub(1);
        let date = &inp.created_at[..10.min(inp.created_at.len())];

        rows_html.push_str(&format!(
            r##"<tr>
                <td><a href="{base}/forms/inputs/{id}">{name}</a></td>
                <td><span class="label-badge" style="--label-color:#1A6B5A">{rs_label}</span></td>
                <td>{ft_name}</td>
                <td class="mono">{row_count}</td>
                <td class="txn-date">{date}</td>
                <td>
                    <form method="POST" action="{base}/forms/inputs/{id}/delete" style="display:inline"
                          onsubmit="return confirm('{delete_confirm}')">
                        <button class="btn-icon btn-icon-danger">{delete_label}</button>
                    </form>
                </td>
            </tr>"##,
            id = inp.id,
            name = inp.name,
        ));
    }

    let table_or_empty = if rows_html.is_empty() {
        format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.inp_empty)
    } else {
        format!(
            r#"<table>
                <thead><tr>
                    <th>{col_name}</th><th>{col_row_set}</th><th>{col_form_type}</th><th>{col_rows}</th><th>{col_date}</th><th></th>
                </tr></thead>
                <tbody>{rows_html}</tbody>
            </table>"#,
            col_name = t.inp_col_name,
            col_row_set = t.inp_col_row_set,
            col_form_type = t.inp_col_form_type,
            col_rows = t.inp_col_rows,
            col_date = t.inp_col_date,
        )
    };

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <div>
                    <h1>{title}</h1>
                    <p>{subtitle}</p>
                </div>
                <a href="{base}/forms/new" class="btn btn-primary">{new_btn}</a>
            </div>
        </div>

        <div class="card">
            {table_or_empty}
        </div>"##,
        title = t.inp_title,
        subtitle = t.inp_subtitle,
        new_btn = t.inp_new_btn,
    );

    Html(render_page(
        &format!("Forms — {}", t.inputs),
        &forms_nav(base, "inputs", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn new_input_page(
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

    let form_types: Vec<FormTypeRow> = sqlx::query_as(
        "SELECT id, name, columns_json FROM form_input_form_types WHERE user_id = ? ORDER BY name ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if row_sets.is_empty() || form_types.is_empty() {
        let msg = if row_sets.is_empty() && form_types.is_empty() {
            t.inp_need_both.to_string()
        } else if row_sets.is_empty() {
            t.inp_need_row_set.to_string()
        } else {
            t.inp_need_form_type.to_string()
        };
        let body = format!(
            r#"<div class="page-header"><h1>{title}</h1></div>
            <div class="card" style="max-width:36rem"><div class="card-body"><p>{msg}</p></div></div>"#,
            title = t.inp_new_title,
        );
        return Html(render_page(
            &format!("Forms — {}", t.inp_new_title),
            &forms_nav(base, "inputs", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let row_sets_json: Vec<serde_json::Value> = row_sets
        .iter()
        .map(|rs| {
            let rows: Vec<&str> = rs.rows.lines().filter(|l| !l.trim().is_empty()).collect();
            serde_json::json!({"id": rs.id, "label": rs.label, "rows": rows})
        })
        .collect();
    let form_types_json: Vec<serde_json::Value> = form_types
        .iter()
        .map(|f| {
            let cols: Vec<ColumnDef> = serde_json::from_str(&f.columns_json).unwrap_or_default();
            serde_json::json!({"id": f.id, "name": f.name, "columns": cols})
        })
        .collect();

    let rs_json = serde_json::to_string(&row_sets_json).unwrap_or_default();
    let ft_json = serde_json::to_string(&form_types_json).unwrap_or_default();

    let mut rs_opts = String::new();
    for rs in &row_sets {
        rs_opts.push_str(&format!(
            r#"<option value="{}">{}</option>"#,
            rs.id, rs.label
        ));
    }

    let mut ft_opts = String::new();
    for f in &form_types {
        ft_opts.push_str(&format!(r#"<option value="{}">{}</option>"#, f.id, f.name));
    }

    let row_label = t.inp_row;
    let select_hint = t.inp_select_hint;
    let col_bool = t.ft_col_bool;

    let body = format!(
        r##"<div class="page-header">
            <h1>{new_title}</h1>
            <p>{new_subtitle}</p>
        </div>

        <div class="card" style="max-width:60rem;">
            <div class="card-body">
                <form method="POST" action="{base}/forms/inputs/create" id="input-form">
                    <div class="form-row" style="align-items:flex-end;gap:1rem;flex-wrap:wrap">
                        <div class="form-group">
                            <label for="row_set_id">{row_set_lbl}</label>
                            <select id="row_set_id" name="row_set_id" required>{rs_opts}</select>
                        </div>
                        <div class="form-group">
                            <label for="form_type_id">{form_type_lbl}</label>
                            <select id="form_type_id" name="form_type_id" required>{ft_opts}</select>
                        </div>
                        <div class="form-group" style="flex:1">
                            <label for="input_name">{name_lbl}</label>
                            <input type="text" id="input_name" name="name" required placeholder="e.g. Week 12 quiz">
                        </div>
                    </div>

                    <div id="grid-container" class="ci-grid-container mt-2"></div>

                    <input type="hidden" name="csv_data" id="csv_data">
                    <button type="submit" class="btn btn-primary mt-2" id="submit-btn">{save_btn}</button>
                </form>
            </div>
        </div>

        <script>
        (function() {{
            var rowSets = {rs_json};
            var formTypes = {ft_json};
            var lblRow = '{row_label}';
            var lblSelectHint = '{select_hint}';
            var lblBool = '{col_bool}';

            var rsSel = document.getElementById('row_set_id');
            var ftSel = document.getElementById('form_type_id');
            var gridContainer = document.getElementById('grid-container');
            var csvInput = document.getElementById('csv_data');
            var form = document.getElementById('input-form');

            function buildGrid() {{
                var rsId = parseInt(rsSel.value);
                var ftId = parseInt(ftSel.value);
                var rs = rowSets.find(function(r) {{ return r.id === rsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                if (!rs || !ft || ft.columns.length === 0) {{
                    gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
                    return;
                }}

                var rows = rs.rows;
                var cols = ft.columns;

                var html = '<table class="ci-input-table"><thead><tr><th class="ci-th-pupil">' + lblRow + '</th>';
                for (var i = 0; i < cols.length; i++) {{
                    html += '<th>' + cols[i].name + '</th>';
                }}
                html += '</tr></thead><tbody>';

                for (var r = 0; r < rows.length; r++) {{
                    html += '<tr><td class="ci-pupil-name">' + rows[r] + '</td>';
                    for (var c = 0; c < cols.length; c++) {{
                        var colType = cols[c].type || cols[c].col_type || 'text';
                        if (colType === 'bool') {{
                            var parts = lblBool.split(' / ');
                            var yes = parts[0] || 'Yes';
                            var no = parts[1] || 'No';
                            html += '<td class="ci-col-bool"><select data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-select">'
                                + '<option value=""></option><option value="' + yes + '">' + yes + '</option><option value="' + no + '">' + no + '</option></select></td>';
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
                var rsId = parseInt(rsSel.value);
                var ftId = parseInt(ftSel.value);
                var rs = rowSets.find(function(r) {{ return r.id === rsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                var maxR = rs ? rs.rows.length - 1 : 0;
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

            rsSel.addEventListener('change', buildGrid);
            ftSel.addEventListener('change', buildGrid);
            buildGrid();

            form.addEventListener('submit', function() {{
                var rsId = parseInt(rsSel.value);
                var ftId = parseInt(ftSel.value);
                var rs = rowSets.find(function(r) {{ return r.id === rsId; }});
                var ft = formTypes.find(function(f) {{ return f.id === ftId; }});
                if (!rs || !ft) return;

                var rows = rs.rows;
                var cols = ft.columns;

                var lines = [];
                var header = [lblRow];
                for (var i = 0; i < cols.length; i++) header.push(csvEscape(cols[i].name));
                lines.push(header.join(','));

                for (var r = 0; r < rows.length; r++) {{
                    var row = [csvEscape(rows[r])];
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
        </script>"##,
        new_title = t.inp_new_title,
        new_subtitle = t.inp_new_subtitle,
        row_set_lbl = t.inp_row_set,
        form_type_lbl = t.inp_form_type,
        name_lbl = t.inp_name,
        save_btn = t.inp_save,
    );

    Html(render_page(
        &format!("Forms — {}", t.inp_new_title),
        &forms_nav(base, "inputs", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct CreateInputForm {
    row_set_id: i64,
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
    super::ops::create_input(
        &state.pool,
        user_id.0,
        Some(form.row_set_id),
        form.form_type_id,
        form.name.trim(),
        &form.csv_data,
    )
    .await
    .ok();
    Redirect::to(&format!("{base}/forms"))
}

async fn view(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let inp: Option<InputRow> = sqlx::query_as(
        "SELECT id, row_set_id, form_type_id, name, csv_data, created_at
         FROM form_input_inputs WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(inp) = inp else {
        return Html(render_page(
            "Forms — Not Found",
            &forms_nav(base, "inputs", lang),
            &format!(
                r#"<div class="empty-state"><p>{}</p></div>"#,
                t.inp_not_found
            ),
            &state.config,
            lang,
        ));
    };

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

    let rs_label: Option<String> = match inp.row_set_id {
        Some(rsid) => sqlx::query_scalar("SELECT label FROM form_input_row_sets WHERE id = ?")
            .bind(rsid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None),
        None => None,
    };
    let ft_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM form_input_form_types WHERE id = ?")
            .bind(inp.form_type_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let date = &inp.created_at[..10.min(inp.created_at.len())];

    let body = format!(
        r##"<div class="page-header">
            <h1>{name}</h1>
            <p>
                <span class="label-badge" style="--label-color:#1A6B5A">{rs_label}</span>
                {ft_name} — {date}
            </p>
        </div>

        <div class="card">
            {table_html}
        </div>

        <div class="mt-2">
            <a href="{base}/forms" class="btn btn-secondary">{back}</a>
        </div>"##,
        name = inp.name,
        rs_label = rs_label.as_deref().unwrap_or("?"),
        ft_name = ft_name.as_deref().unwrap_or("?"),
        back = t.inp_back,
    );

    Html(render_page(
        &format!("Forms — {}", inp.name),
        &forms_nav(base, "inputs", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("DELETE FROM form_input_inputs WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/forms"))
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

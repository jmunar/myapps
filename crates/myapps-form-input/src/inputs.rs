use axum::{
    Extension, Form, Router,
    extract::Path,
    http::StatusCode,
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
        .route("/inputs/{id}/cell", post(update_cell))
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
    fixed_rows: bool,
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
        "SELECT id, name, columns_json, fixed_rows FROM form_input_form_types WHERE user_id = ?",
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
        "SELECT id, name, columns_json, fixed_rows FROM form_input_form_types WHERE user_id = ? ORDER BY name ASC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if form_types.is_empty() {
        let body = format!(
            r#"<div class="page-header"><h1>{title}</h1></div>
            <div class="card" style="max-width:36rem"><div class="card-body"><p>{msg}</p></div></div>"#,
            title = t.inp_new_title,
            msg = t.inp_need_form_type,
        );
        return Html(render_page(
            &format!("Forms — {}", t.inp_new_title),
            &forms_nav(base, "inputs", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let any_fixed = form_types.iter().any(|f| f.fixed_rows);
    if row_sets.is_empty() && any_fixed && form_types.iter().all(|f| f.fixed_rows) {
        let body = format!(
            r#"<div class="page-header"><h1>{title}</h1></div>
            <div class="card" style="max-width:36rem"><div class="card-body"><p>{msg}</p></div></div>"#,
            title = t.inp_new_title,
            msg = t.inp_need_row_set,
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
            serde_json::json!({
                "id": f.id,
                "name": f.name,
                "columns": cols,
                "fixed_rows": f.fixed_rows,
            })
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
    let add_row_label = t.inp_add_row;
    let remove_row_label = t.inp_remove_row;
    let no_rows_yet = t.inp_no_rows_yet;
    let need_row_set = t.inp_need_row_set;
    let link_default_text = t.link_default_text;
    let link_add_btn = t.link_add_btn;
    let modal_html = render_link_modal(t);

    let body = format!(
        r##"<div class="page-header">
            <h1>{new_title}</h1>
            <p>{new_subtitle}</p>
        </div>

        <div class="card" style="max-width:60rem;">
            <div class="card-body">
                <form method="POST" action="{base}/forms/inputs/create" id="input-form">
                    <div class="form-row" style="align-items:flex-end;gap:1rem;flex-wrap:wrap">
                        <div class="form-group" id="row-set-group">
                            <label for="row_set_id">{row_set_lbl}</label>
                            <select id="row_set_id" name="row_set_id">{rs_opts}</select>
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

                    <div id="row-set-warning" class="text-secondary mt-2" style="display:none">{need_row_set}</div>
                    <div id="grid-container" class="ci-grid-container mt-2"></div>
                    <button type="button" id="add-row-btn" class="btn btn-secondary btn-sm mt-1" style="display:none">{add_row_label}</button>

                    <input type="hidden" name="csv_data" id="csv_data">
                    <button type="submit" class="btn btn-primary mt-2" id="submit-btn">{save_btn}</button>
                </form>
            </div>
        </div>

        {modal_html}

        <script>
        (function() {{
            var rowSets = {rs_json};
            var formTypes = {ft_json};
            var lblRow = '{row_label}';
            var lblSelectHint = '{select_hint}';
            var lblBool = '{col_bool}';
            var lblRemoveRow = '{remove_row_label}';
            var lblNoRowsYet = '{no_rows_yet}';
            var lblLinkDefault = '{link_default_text}';
            var lblLinkAdd = '{link_add_btn}';

            var rsSel = document.getElementById('row_set_id');
            var ftSel = document.getElementById('form_type_id');
            var rsGroup = document.getElementById('row-set-group');
            var rsWarning = document.getElementById('row-set-warning');
            var gridContainer = document.getElementById('grid-container');
            var addRowBtn = document.getElementById('add-row-btn');
            var submitBtn = document.getElementById('submit-btn');
            var csvInput = document.getElementById('csv_data');
            var form = document.getElementById('input-form');

            // dynamic-mode state: array of arrays of strings; built from DOM on submit
            var dynamicRowCount = 0;

            function currentFormType() {{
                var ftId = parseInt(ftSel.value);
                return formTypes.find(function(f) {{ return f.id === ftId; }});
            }}

            function currentRowSet() {{
                var rsId = parseInt(rsSel.value);
                return rowSets.find(function(r) {{ return r.id === rsId; }});
            }}

            function cellHtml(r, c, colType) {{
                if (colType === 'bool') {{
                    var parts = lblBool.split(' / ');
                    var yes = parts[0] || 'Yes';
                    var no = parts[1] || 'No';
                    return '<td class="ci-col-bool"><select data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-select">'
                        + '<option value=""></option><option value="' + yes + '">' + yes + '</option><option value="' + no + '">' + no + '</option></select></td>';
                }} else if (colType === 'number') {{
                    return '<td class="ci-col-number"><input type="number" step="any" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input" inputmode="decimal"></td>';
                }} else if (colType === 'link') {{
                    return '<td class="ci-col-link">'
                        + '<input type="hidden" data-r="' + r + '" data-c="' + c + '" class="ci-cell" value="">'
                        + '<button type="button" class="ci-link-btn" onclick="window.openLinkModal(this)">' + lblLinkAdd + '</button>'
                        + '</td>';
                }}
                return '<td><input type="text" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input"></td>';
            }}

            function buildFixedGrid(rs, ft) {{
                var rows = rs.rows;
                var cols = ft.columns;
                var html = '<table class="ci-input-table"><thead><tr><th class="ci-th-pupil">' + lblRow + '</th>';
                for (var i = 0; i < cols.length; i++) html += '<th>' + cols[i].name + '</th>';
                html += '</tr></thead><tbody>';
                for (var r = 0; r < rows.length; r++) {{
                    html += '<tr><td class="ci-pupil-name">' + rows[r] + '</td>';
                    for (var c = 0; c < cols.length; c++) {{
                        var colType = cols[c].type || cols[c].col_type || 'text';
                        html += cellHtml(r, c, colType);
                    }}
                    html += '</tr>';
                }}
                html += '</tbody></table>';
                gridContainer.innerHTML = html;
            }}

            function dynamicRowHtml(r, cols) {{
                var html = '<tr data-row="' + r + '">';
                for (var c = 0; c < cols.length; c++) {{
                    var colType = cols[c].type || cols[c].col_type || 'text';
                    html += cellHtml(r, c, colType);
                }}
                html += '<td style="padding:0 0.4rem"><button type="button" class="btn-icon btn-icon-danger remove-row-btn" data-row="' + r + '" title="' + lblRemoveRow + '">×</button></td>';
                html += '</tr>';
                return html;
            }}

            function buildDynamicGrid(ft) {{
                var cols = ft.columns;
                if (cols.length === 0) {{
                    gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
                    return;
                }}
                var html = '<table class="ci-input-table"><thead><tr>';
                for (var i = 0; i < cols.length; i++) html += '<th>' + cols[i].name + '</th>';
                html += '<th></th></tr></thead><tbody id="dynamic-rows">';
                html += dynamicRowHtml(0, cols);
                html += '</tbody></table>';
                gridContainer.innerHTML = html;
                dynamicRowCount = 1;
                wireRemoveButtons(cols);
            }}

            function wireRemoveButtons(cols) {{
                gridContainer.querySelectorAll('.remove-row-btn').forEach(function(btn) {{
                    btn.onclick = function() {{
                        var tbody = document.getElementById('dynamic-rows');
                        if (!tbody) return;
                        if (tbody.children.length <= 1) return;
                        var row = btn.closest('tr');
                        if (row) row.remove();
                    }};
                }});
            }}

            function applyMode() {{
                var ft = currentFormType();
                if (!ft) {{
                    gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
                    addRowBtn.style.display = 'none';
                    return;
                }}
                if (ft.fixed_rows) {{
                    rsGroup.style.display = '';
                    rsSel.required = true;
                    addRowBtn.style.display = 'none';
                    if (rowSets.length === 0) {{
                        rsWarning.style.display = '';
                        gridContainer.innerHTML = '';
                        submitBtn.disabled = true;
                        return;
                    }}
                    rsWarning.style.display = 'none';
                    submitBtn.disabled = false;
                    var rs = currentRowSet();
                    if (!rs || ft.columns.length === 0) {{
                        gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
                        return;
                    }}
                    buildFixedGrid(rs, ft);
                }} else {{
                    rsGroup.style.display = 'none';
                    rsSel.required = false;
                    rsWarning.style.display = 'none';
                    addRowBtn.style.display = '';
                    submitBtn.disabled = false;
                    buildDynamicGrid(ft);
                }}
            }}

            addRowBtn.addEventListener('click', function() {{
                var ft = currentFormType();
                if (!ft || ft.columns.length === 0) return;
                var tbody = document.getElementById('dynamic-rows');
                if (!tbody) return;
                tbody.insertAdjacentHTML('beforeend', dynamicRowHtml(dynamicRowCount, ft.columns));
                dynamicRowCount++;
                wireRemoveButtons(ft.columns);
            }});

            rsSel.addEventListener('change', applyMode);
            ftSel.addEventListener('change', applyMode);
            applyMode();

            form.addEventListener('submit', function(e) {{
                var ft = currentFormType();
                if (!ft) return;
                var cols = ft.columns;
                var lines = [];

                if (ft.fixed_rows) {{
                    var rs = currentRowSet();
                    if (!rs) {{ e.preventDefault(); return; }}
                    var rows = rs.rows;
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
                }} else {{
                    var header2 = [];
                    for (var i2 = 0; i2 < cols.length; i2++) header2.push(csvEscape(cols[i2].name));
                    lines.push(header2.join(','));
                    var trs = gridContainer.querySelectorAll('#dynamic-rows tr');
                    trs.forEach(function(tr) {{
                        var rowVals = [];
                        for (var c2 = 0; c2 < cols.length; c2++) {{
                            var cell2 = tr.querySelector('[data-c="' + c2 + '"]');
                            rowVals.push(csvEscape(cell2 ? cell2.value : ''));
                        }}
                        lines.push(rowVals.join(','));
                    }});
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

            // ── Link modal ─────────────────────────────────────────────
            var modal = document.getElementById('link-modal');
            var modalForm = document.getElementById('link-modal-form');
            var modalUrl = document.getElementById('link-modal-url');
            var modalText = document.getElementById('link-modal-text');
            var modalCancel = document.getElementById('link-modal-cancel');
            var modalActiveBtn = null;

            function parseLinkValue(v) {{
                if (!v) return ['', ''];
                var i = v.indexOf('|');
                if (i < 0) return [v, ''];
                return [v.slice(0, i), v.slice(i + 1)];
            }}
            function encodeLinkValue(url, text) {{
                if (!url) return '';
                return text ? url + '|' + text : url;
            }}
            function buttonLabel(text) {{
                return text || lblLinkDefault;
            }}

            window.openLinkModal = function(btn) {{
                modalActiveBtn = btn;
                var hidden = btn.previousElementSibling;
                var current = hidden ? hidden.value : '';
                var parsed = parseLinkValue(current);
                modalUrl.value = parsed[0];
                modalText.value = parsed[1];
                if (modal && modal.showModal) modal.showModal();
            }};

            if (modalCancel) modalCancel.addEventListener('click', function() {{
                modalActiveBtn = null;
                if (modal && modal.close) modal.close();
            }});

            if (modalForm) modalForm.addEventListener('submit', function(e) {{
                e.preventDefault();
                if (!modalActiveBtn) {{ if (modal && modal.close) modal.close(); return; }}
                var url = modalUrl.value.trim();
                if (!url) return;
                var text = modalText.value.trim();
                var hidden = modalActiveBtn.previousElementSibling;
                if (hidden) hidden.value = encodeLinkValue(url, text);
                modalActiveBtn.textContent = buttonLabel(text);
                modalActiveBtn = null;
                if (modal && modal.close) modal.close();
            }});
        }})();
        </script>"##,
        new_title = t.inp_new_title,
        new_subtitle = t.inp_new_subtitle,
        row_set_lbl = t.inp_row_set,
        form_type_lbl = t.inp_form_type,
        name_lbl = t.inp_name,
        save_btn = t.inp_save,
        add_row_label = add_row_label,
        remove_row_label = remove_row_label,
        no_rows_yet = no_rows_yet,
        need_row_set = need_row_set,
        link_default_text = link_default_text,
        link_add_btn = link_add_btn,
        modal_html = modal_html,
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
    #[serde(default)]
    row_set_id: Option<String>,
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

    let fixed_rows: bool = sqlx::query_scalar(
        "SELECT fixed_rows FROM form_input_form_types WHERE id = ? AND user_id = ?",
    )
    .bind(form.form_type_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    let row_set_id: Option<i64> = if fixed_rows {
        form.row_set_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok())
    } else {
        None
    };

    super::ops::create_input(
        &state.pool,
        user_id.0,
        row_set_id,
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

    // Look up the form type's columns so the JS knows each column's input control.
    let ft_row: Option<(String, String)> =
        sqlx::query_as("SELECT name, columns_json FROM form_input_form_types WHERE id = ?")
            .bind(inp.form_type_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let (ft_name, columns_json) = ft_row
        .map(|(n, j)| (Some(n), j))
        .unwrap_or((None, "[]".to_string()));
    let ft_columns: Vec<ColumnDef> = serde_json::from_str(&columns_json).unwrap_or_default();

    let highlight_first_col = inp.row_set_id.is_some();
    let lines: Vec<Vec<String>> = inp.csv_data.lines().map(parse_csv_line).collect();

    // Build the grid. The first column is the row identifier when fixed-row mode is on,
    // mirroring the new-input page's layout. Editable cells get data-row/data-col/data-type
    // so the JS double-click handler knows which control to spawn.
    let mut table_html = String::from(r#"<table class="ci-input-table"><thead><tr>"#);
    if let Some(header) = lines.first() {
        for (i, col) in header.iter().enumerate() {
            if i == 0 && highlight_first_col {
                table_html.push_str(&format!(r#"<th class="ci-th-pupil">{col}</th>"#));
            } else {
                table_html.push_str(&format!("<th>{col}</th>"));
            }
        }
    }
    table_html.push_str("</tr></thead><tbody>");
    for (r, line) in lines.iter().enumerate().skip(1) {
        table_html.push_str("<tr>");
        for (c, field) in line.iter().enumerate() {
            if c == 0 && highlight_first_col {
                table_html.push_str(&format!(r#"<td class="ci-pupil-name">{field}</td>"#));
            } else {
                // Editable. Cell type comes from the form-type column at the matching
                // index. For fixed-row mode the leading column is the row id, so user
                // columns are offset by 1.
                let col_idx = if highlight_first_col { c - 1 } else { c };
                let col_type = ft_columns
                    .get(col_idx)
                    .map(|cd| cd.col_type.as_str())
                    .unwrap_or("text");
                let cell_class = match col_type {
                    "number" => "ci-cell-editable ci-col-number",
                    "bool" => "ci-cell-editable ci-col-bool",
                    "link" => "ci-cell-editable ci-col-link",
                    _ => "ci-cell-editable",
                };
                if col_type == "link" {
                    let (url, text) = parse_link_value(field);
                    let display = if url.is_empty() {
                        String::new()
                    } else {
                        let display_text = if text.is_empty() {
                            t.link_default_text
                        } else {
                            text
                        };
                        format!(
                            r#"<a href="{href}" target="_blank" rel="noopener">{txt}</a>"#,
                            href = html_escape(url),
                            txt = html_escape(display_text),
                        )
                    };
                    table_html.push_str(&format!(
                        r#"<td class="{cell_class}" data-row="{r}" data-col="{c}" data-type="link" data-value="{value}">{display}</td>"#,
                        value = html_escape(field),
                    ));
                } else {
                    table_html.push_str(&format!(
                        r#"<td class="{cell_class}" data-row="{r}" data-col="{c}" data-type="{col_type}">{field}</td>"#,
                    ));
                }
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

    let date = &inp.created_at[..10.min(inp.created_at.len())];

    let rs_badge = match rs_label.as_deref() {
        Some(label) => {
            format!(r#"<span class="label-badge" style="--label-color:#1A6B5A">{label}</span> "#)
        }
        None => String::new(),
    };

    let col_bool = t.ft_col_bool;
    let link_default_text = t.link_default_text;
    let modal_html = render_link_modal(t);

    let body = format!(
        r##"<div class="page-header">
            <h1>{name}</h1>
            <p>
                {rs_badge}{ft_name} — {date}
            </p>
        </div>

        <div class="card">
            <div class="ci-grid-container">
                {table_html}
            </div>
        </div>

        <div class="mt-2">
            <a href="{base}/forms" class="btn btn-secondary">{back}</a>
        </div>

        {modal_html}

        <script>
        (function() {{
            var lblBool = '{col_bool}';
            var lblLinkDefault = '{link_default_text}';
            var saveUrl = '{base}/forms/inputs/{id}/cell';

            // ── Link modal ─────────────────────────────────────────────
            var modal = document.getElementById('link-modal');
            var modalForm = document.getElementById('link-modal-form');
            var modalUrl = document.getElementById('link-modal-url');
            var modalText = document.getElementById('link-modal-text');
            var modalCancel = document.getElementById('link-modal-cancel');
            var modalActiveCell = null;

            function parseLinkValue(v) {{
                if (!v) return ['', ''];
                var i = v.indexOf('|');
                if (i < 0) return [v, ''];
                return [v.slice(0, i), v.slice(i + 1)];
            }}
            function encodeLinkValue(url, text) {{
                if (!url) return '';
                return text ? url + '|' + text : url;
            }}
            function renderLinkCellHtml(value) {{
                var parsed = parseLinkValue(value);
                var url = parsed[0], text = parsed[1] || lblLinkDefault;
                if (!url) return '';
                var a = document.createElement('a');
                a.href = url;
                a.target = '_blank';
                a.rel = 'noopener';
                a.textContent = text;
                return a.outerHTML;
            }}

            if (modalCancel) modalCancel.addEventListener('click', function() {{
                modalActiveCell = null;
                if (modal && modal.close) modal.close();
            }});
            if (modalForm) modalForm.addEventListener('submit', function(e) {{
                e.preventDefault();
                if (!modalActiveCell) {{ if (modal && modal.close) modal.close(); return; }}
                var url = modalUrl.value.trim();
                if (!url) return;
                var text = modalText.value.trim();
                var newValue = encodeLinkValue(url, text);
                var cell = modalActiveCell;
                modalActiveCell = null;
                saveLink(cell, newValue);
                if (modal && modal.close) modal.close();
            }});

            function saveLink(cell, newValue) {{
                var oldValue = cell.dataset.value || '';
                cell.dataset.value = newValue;
                cell.innerHTML = renderLinkCellHtml(newValue);
                var body = 'row=' + encodeURIComponent(cell.dataset.row)
                    + '&col=' + encodeURIComponent(cell.dataset.col)
                    + '&value=' + encodeURIComponent(newValue);
                fetch(saveUrl, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/x-www-form-urlencoded' }},
                    body: body,
                    credentials: 'same-origin',
                }}).then(function(res) {{
                    if (!res.ok) {{
                        alert('Save failed (' + res.status + ')');
                        cell.dataset.value = oldValue;
                        cell.innerHTML = renderLinkCellHtml(oldValue);
                    }}
                }}).catch(function() {{
                    alert('Save failed (network error)');
                    cell.dataset.value = oldValue;
                    cell.innerHTML = renderLinkCellHtml(oldValue);
                }});
            }}

            // ── Cell editing ───────────────────────────────────────────
            document.querySelectorAll('.ci-cell-editable').forEach(function(cell) {{
                cell.addEventListener('dblclick', function() {{
                    if (cell.classList.contains('ci-cell-editing')) return;
                    var colType = cell.dataset.type || 'text';
                    if (colType === 'link') {{
                        modalActiveCell = cell;
                        var parsed = parseLinkValue(cell.dataset.value || '');
                        modalUrl.value = parsed[0];
                        modalText.value = parsed[1];
                        if (modal && modal.showModal) modal.showModal();
                        return;
                    }}
                    startEdit(cell);
                }});
            }});

            function startEdit(cell) {{
                var oldValue = cell.textContent;
                var colType = cell.dataset.type || 'text';
                cell.classList.add('ci-cell-editing');
                cell.dataset.oldValue = oldValue;
                var control;
                if (colType === 'bool') {{
                    var parts = lblBool.split(' / ');
                    var yes = parts[0] || 'Yes';
                    var no = parts[1] || 'No';
                    control = document.createElement('select');
                    control.className = 'ci-cell ci-cell-select';
                    control.innerHTML = '<option value=""></option>'
                        + '<option value="' + yes + '">' + yes + '</option>'
                        + '<option value="' + no + '">' + no + '</option>';
                    control.value = oldValue;
                }} else if (colType === 'number') {{
                    control = document.createElement('input');
                    control.type = 'number';
                    control.step = 'any';
                    control.inputMode = 'decimal';
                    control.className = 'ci-cell ci-cell-input';
                    control.value = oldValue;
                }} else {{
                    control = document.createElement('input');
                    control.type = 'text';
                    control.className = 'ci-cell ci-cell-input';
                    control.value = oldValue;
                }}
                cell.textContent = '';
                cell.appendChild(control);
                control.focus();
                if (control.select) control.select();

                var done = false;
                function finish(commit) {{
                    if (done) return;
                    done = true;
                    if (commit) {{
                        save(cell, control.value);
                    }} else {{
                        cell.textContent = cell.dataset.oldValue || '';
                        cell.classList.remove('ci-cell-editing');
                        delete cell.dataset.oldValue;
                    }}
                }}
                control.addEventListener('keydown', function(e) {{
                    if (e.key === 'Enter') {{ e.preventDefault(); finish(true); }}
                    else if (e.key === 'Escape') {{ e.preventDefault(); finish(false); }}
                }});
                control.addEventListener('blur', function() {{ finish(true); }});
            }}

            function save(cell, newValue) {{
                var body = 'row=' + encodeURIComponent(cell.dataset.row)
                    + '&col=' + encodeURIComponent(cell.dataset.col)
                    + '&value=' + encodeURIComponent(newValue);
                fetch(saveUrl, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/x-www-form-urlencoded' }},
                    body: body,
                    credentials: 'same-origin',
                }}).then(function(res) {{
                    if (res.ok) {{
                        cell.textContent = newValue;
                    }} else {{
                        alert('Save failed (' + res.status + ')');
                        cell.textContent = cell.dataset.oldValue || '';
                    }}
                }}).catch(function() {{
                    alert('Save failed (network error)');
                    cell.textContent = cell.dataset.oldValue || '';
                }}).finally(function() {{
                    cell.classList.remove('ci-cell-editing');
                    delete cell.dataset.oldValue;
                }});
            }}
        }})();
        </script>"##,
        name = inp.name,
        ft_name = ft_name.as_deref().unwrap_or("?"),
        back = t.inp_back,
        id = inp.id,
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

/// Shared `<dialog>` markup for editing link cells. Both the new-input page and
/// the view page render this so the JS on either page can call `showModal()`.
fn render_link_modal(t: &super::i18n::Translations) -> String {
    format!(
        r##"<dialog id="link-modal" class="ci-link-modal">
            <form id="link-modal-form" method="dialog">
                <h3 style="margin-top:0">{title}</h3>
                <div class="form-group">
                    <label for="link-modal-url">{url_lbl}</label>
                    <input type="url" id="link-modal-url" required>
                </div>
                <div class="form-group">
                    <label for="link-modal-text">{text_lbl}</label>
                    <input type="text" id="link-modal-text" placeholder="{default_text}">
                </div>
                <div style="display:flex;gap:0.5rem;justify-content:flex-end;margin-top:0.75rem">
                    <button type="button" id="link-modal-cancel" class="btn btn-secondary">{cancel}</button>
                    <button type="submit" id="link-modal-save" class="btn btn-primary">{save}</button>
                </div>
            </form>
        </dialog>"##,
        title = t.link_modal_title,
        url_lbl = t.link_modal_url,
        text_lbl = t.link_modal_text,
        default_text = t.link_default_text,
        cancel = t.ft_cancel,
        save = t.ft_save,
    )
}

/// Split a stored `url|text` link cell into `(url, text)`. Splits on the first
/// pipe so additional pipes in the display text are preserved. URLs that
/// genuinely contain `|` are expected to be URL-encoded as `%7C`.
fn parse_link_value(value: &str) -> (&str, &str) {
    match value.split_once('|') {
        Some((u, t)) => (u, t),
        None => (value, ""),
    }
}

/// HTML-escape a string for use in attribute or text content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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

/// Mirror of the JS csvEscape: quote when the value contains a comma, quote, or newline.
fn csv_escape(val: &str) -> String {
    if val.is_empty() {
        return String::new();
    }
    if val.contains(',') || val.contains('"') || val.contains('\n') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

fn serialize_csv_line(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| csv_escape(f))
        .collect::<Vec<_>>()
        .join(",")
}

/// Replace a single cell at (row, col) of `csv_data` with `new_value` and
/// return the rewritten CSV. `row == 0` is the header and is rejected.
fn update_csv_cell(
    csv_data: &str,
    row: usize,
    col: usize,
    new_value: &str,
) -> Result<String, &'static str> {
    if row == 0 {
        return Err("header row is not editable");
    }
    let mut lines: Vec<Vec<String>> = csv_data.lines().map(parse_csv_line).collect();
    if row >= lines.len() {
        return Err("row out of range");
    }
    let line = &mut lines[row];
    if col >= line.len() {
        return Err("col out of range");
    }
    line[col] = new_value.to_string();
    Ok(lines
        .into_iter()
        .map(|l| serialize_csv_line(&l))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[derive(Deserialize)]
struct UpdateCellForm {
    row: usize,
    col: usize,
    #[serde(default)]
    value: String,
}

async fn update_cell(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<UpdateCellForm>,
) -> impl IntoResponse {
    let inp: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT csv_data, row_set_id FROM form_input_inputs WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((csv_data, row_set_id)) = inp else {
        return StatusCode::NOT_FOUND;
    };

    // For fixed-row inputs the leading column is the row identifier — not editable.
    if row_set_id.is_some() && form.col == 0 {
        return StatusCode::BAD_REQUEST;
    }

    let updated = match update_csv_cell(&csv_data, form.row, form.col, &form.value) {
        Ok(s) => s,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let res = sqlx::query("UPDATE form_input_inputs SET csv_data = ? WHERE id = ? AND user_id = ?")
        .bind(&updated)
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await;

    match res {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("DB update failed: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

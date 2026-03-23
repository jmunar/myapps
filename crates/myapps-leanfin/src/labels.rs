use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::dashboard::leanfin_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels))
        .route("/labels/create", post(create_label))
        .route("/labels/{id}/delete", post(delete_label))
        .route("/labels/{id}/edit", post(edit_label))
        .route("/labels/{id}/rules", get(list_rules))
        .route("/labels/{id}/rules/create", post(create_rule))
        .route(
            "/labels/{label_id}/rules/{rule_id}/delete",
            post(delete_rule),
        )
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
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let labels: Vec<LabelRow> = sqlx::query_as(
        r#"SELECT l.id, l.name, l.color,
                  (SELECT COUNT(*) FROM leanfin_label_rules WHERE label_id = l.id) AS rule_count,
                  (SELECT COUNT(*) FROM leanfin_allocations WHERE label_id = l.id) AS txn_count
           FROM leanfin_labels l
           WHERE l.user_id = ?
           ORDER BY l.name"#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let lbl_rules = t.lbl_rules;
    let lbl_edit = t.lbl_edit;
    let lbl_delete = t.lbl_delete;
    let lbl_delete_confirm = t.lbl_delete_confirm;
    let lbl_save = t.lbl_save;

    let mut items = String::new();
    for l in &labels {
        let color = l.color.as_deref().unwrap_or("#6B6B6B");
        let id = l.id;
        let name = &l.name;
        let rules_url = format!("{base}/leanfin/labels/{id}/rules");
        let delete_url = format!("{base}/leanfin/labels/{id}/delete");
        let edit_url = format!("{base}/leanfin/labels/{id}/edit");

        items.push_str(&format!(
            concat!(
                r##"<div class="label-item" id="label-{id}">"##,
                r##"<div class="label-item-info">"##,
                r##"<span class="label-badge" style="--label-color:{color}">{name}</span>"##,
                r##"<span class="text-secondary text-sm">{rule_count}r / {txn_count}t</span>"##,
                r##"</div>"##,
                r##"<div class="label-item-actions">"##,
                r##"<button class="btn-icon" hx-get="{rules_url}" hx-target="#rules-{id}" hx-swap="innerHTML">{lbl_rules}</button>"##,
                r##"<button class="btn-icon" onclick="this.closest('.label-item').querySelector('.label-edit-form').toggleAttribute('hidden')">{lbl_edit}</button>"##,
                r##"<form method="POST" action="{delete_url}" style="display:inline" onsubmit="return confirm('{lbl_delete_confirm}')">"##,
                r##"<button class="btn-icon btn-icon-danger">{lbl_delete}</button>"##,
                r##"</form>"##,
                r##"</div>"##,
                r##"<form method="POST" action="{edit_url}" class="label-edit-form" hidden>"##,
                r##"<input type="text" name="name" value="{name}" required>"##,
                r##"<input type="color" name="color" value="{color}">"##,
                r##"<button type="submit" class="btn btn-primary btn-sm">{lbl_save}</button>"##,
                r##"</form>"##,
                r##"<div id="rules-{id}" class="rules-panel-container"></div>"##,
                r##"</div>"##,
            ),
            id = id,
            name = name,
            color = color,
            rule_count = l.rule_count,
            txn_count = l.txn_count,
            rules_url = rules_url,
            delete_url = delete_url,
            edit_url = edit_url,
            lbl_rules = lbl_rules,
            lbl_edit = lbl_edit,
            lbl_delete = lbl_delete,
            lbl_delete_confirm = lbl_delete_confirm,
            lbl_save = lbl_save,
        ));
    }

    if items.is_empty() {
        items = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.lbl_no_labels
        );
    }

    let default_color = "#4CAF50";
    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="card" style="max-width:36rem;">
            <div class="card-header">
                <h2>{your_labels}</h2>
            </div>
            <div class="card-body">
                <div class="label-list">{items}</div>
            </div>
        </div>

        <div class="card mt-2" style="max-width:36rem;">
            <div class="card-header">
                <h2>{create}</h2>
            </div>
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/labels/create" class="label-create-form">
                    <div class="form-row">
                        <div class="form-group" style="flex:1">
                            <label for="name">{lbl_name}</label>
                            <input type="text" id="name" name="name" required placeholder="e.g. Groceries">
                        </div>
                        <div class="form-group">
                            <label for="color">{lbl_color}</label>
                            <input type="color" id="color" name="color" value="{default_color}">
                        </div>
                    </div>
                    <button type="submit">{create_btn}</button>
                </form>
            </div>
        </div>"#,
        title = t.lbl_title,
        subtitle = t.lbl_subtitle,
        your_labels = t.lbl_your_labels,
        create = t.lbl_create,
        lbl_name = t.lbl_name,
        lbl_color = t.lbl_color,
        create_btn = t.lbl_create_btn,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.labels),
        &leanfin_nav(base, "labels", lang),
        &body,
        &state.config,
        lang,
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
    if let Err(e) =
        sqlx::query("INSERT INTO leanfin_labels (user_id, name, color) VALUES (?, ?, ?)")
            .bind(user_id.0)
            .bind(&form.name)
            .bind(&form.color)
            .execute(&state.pool)
            .await
    {
        tracing::error!("Failed to create label: {e}");
    }
    Redirect::to(&format!("{base}/leanfin/labels"))
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
    sqlx::query("UPDATE leanfin_labels SET name = ?, color = ? WHERE id = ? AND user_id = ?")
        .bind(&form.name)
        .bind(&form.color)
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/leanfin/labels"))
}

// ── Delete label ─────────────────────────────────────────────

async fn delete_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("DELETE FROM leanfin_labels WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/leanfin/labels"))
}

// ── Label rules (HTMX fragments) ────────────────────────────

#[derive(sqlx::FromRow)]
struct RuleRow {
    id: i64,
    field: String,
    pattern: String,
    priority: i64,
}

fn render_rules_panel(base: &str, label_id: i64, rules: &[RuleRow], lang: Lang) -> String {
    let t = super::i18n::t(lang);

    let lbl_delete = t.lbl_delete;
    let lbl_delete_rule_confirm = t.lbl_delete_rule_confirm;

    let mut rows = String::new();
    for r in rules {
        let delete_url = format!("{base}/leanfin/labels/{label_id}/rules/{}/delete", r.id);
        rows.push_str(&format!(
            concat!(
                r##"<div class="rule-row">"##,
                r##"<span class="rule-field">{field}</span>"##,
                r##"<span class="rule-pattern">{contains} &ldquo;<strong>{pattern}</strong>&rdquo;</span>"##,
                r##"<span class="rule-priority text-secondary text-sm">p{priority}</span>"##,
                r##"<form method="POST" action="{delete_url}" "##,
                r##"hx-post="{delete_url}" "##,
                r##"hx-target="#rules-{label_id}" "##,
                r##"hx-swap="innerHTML" "##,
                r##"hx-confirm="{lbl_delete_rule_confirm}" "##,
                r##"style="display:inline">"##,
                r##"<button class="btn-icon btn-icon-danger btn-sm">{lbl_delete}</button>"##,
                r##"</form>"##,
                r##"</div>"##,
            ),
            field = r.field,
            pattern = r.pattern,
            priority = r.priority,
            delete_url = delete_url,
            label_id = label_id,
            contains = t.lbl_contains,
            lbl_delete_rule_confirm = lbl_delete_rule_confirm,
            lbl_delete = lbl_delete,
        ));
    }

    if rows.is_empty() {
        rows = format!(
            r##"<p class="text-secondary text-sm" style="padding:0.25rem 0">{}</p>"##,
            t.lbl_no_rules
        );
    }

    let create_url = format!("{base}/leanfin/labels/{label_id}/rules/create");
    format!(
        concat!(
            r##"<div class="rules-panel">"##,
            r##"<div class="rules-panel-header">"##,
            r##"<span class="text-sm" style="font-weight:600;text-transform:uppercase;letter-spacing:0.04em;color:var(--text-secondary)">{auto_rules}</span>"##,
            r##"</div>"##,
            r##"<div class="rules-list">{rows}</div>"##,
            r##"<form class="rule-add-form" method="POST" action="{create_url}" "##,
            r##"hx-post="{create_url}" "##,
            r##"hx-target="#rules-{label_id}" "##,
            r##"hx-swap="innerHTML">"##,
            r##"<select name="field" required>"##,
            r##"<option value="counterparty">{counterparty}</option>"##,
            r##"<option value="description">{description}</option>"##,
            r##"</select>"##,
            r##"<input type="text" name="pattern" placeholder="{contains}..." required style="flex:1">"##,
            r##"<input type="number" name="priority" value="0" title="{priority}" style="width:3.5rem;text-align:center">"##,
            r##"<button type="submit" class="btn btn-primary btn-sm">{add_rule}</button>"##,
            r##"</form>"##,
            r##"</div>"##,
        ),
        rows = rows,
        create_url = create_url,
        label_id = label_id,
        auto_rules = t.lbl_auto_rules,
        counterparty = t.lbl_counterparty,
        description = t.lbl_description,
        contains = t.lbl_contains,
        priority = t.lbl_priority,
        add_rule = t.lbl_add_rule,
    )
}

async fn list_rules(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(label_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

    let owns = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM leanfin_labels WHERE id = ? AND user_id = ?",
    )
    .bind(label_id)
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if owns == 0 {
        return Html(String::new());
    }

    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT id, field, pattern, priority FROM leanfin_label_rules WHERE label_id = ? ORDER BY priority DESC, id",
    )
    .bind(label_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Html(render_rules_panel(base, label_id, &rules, lang))
}

// ── Create rule ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateRuleForm {
    field: String,
    pattern: String,
    priority: Option<i64>,
}

async fn create_rule(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(label_id): Path<i64>,
    Form(form): Form<CreateRuleForm>,
) -> Html<String> {
    let base = &state.config.base_path;

    let owns = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM leanfin_labels WHERE id = ? AND user_id = ?",
    )
    .bind(label_id)
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if owns == 0 {
        return Html(String::new());
    }

    if form.field != "description" && form.field != "counterparty" {
        return Html(String::new());
    }

    let priority = form.priority.unwrap_or(0);

    if let Err(e) = sqlx::query(
        "INSERT INTO leanfin_label_rules (label_id, field, pattern, priority) VALUES (?, ?, ?, ?)",
    )
    .bind(label_id)
    .bind(&form.field)
    .bind(&form.pattern)
    .bind(priority)
    .execute(&state.pool)
    .await
    {
        tracing::error!("Failed to create rule: {e}");
    }

    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT id, field, pattern, priority FROM leanfin_label_rules WHERE label_id = ? ORDER BY priority DESC, id",
    )
    .bind(label_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Html(render_rules_panel(base, label_id, &rules, lang))
}

// ── Delete rule ─────────────────────────────────────────────

async fn delete_rule(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path((label_id, rule_id)): Path<(i64, i64)>,
) -> Html<String> {
    let base = &state.config.base_path;

    sqlx::query(
        r#"DELETE FROM leanfin_label_rules
           WHERE id = ? AND label_id IN (SELECT id FROM leanfin_labels WHERE id = ? AND user_id = ?)"#,
    )
    .bind(rule_id)
    .bind(label_id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT id, field, pattern, priority FROM leanfin_label_rules WHERE label_id = ? ORDER BY priority DESC, id",
    )
    .bind(label_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Html(render_rules_panel(base, label_id, &rules, lang))
}

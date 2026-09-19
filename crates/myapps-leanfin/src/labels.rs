use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::colors::group_color;
use super::dashboard::leanfin_nav;
use myapps_core::auth::UserId;
use myapps_core::components::html_escape;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

/// Name the default group is seeded with. It is only a default: the group is
/// renameable like any other, and `is_default` — never the name — is what marks
/// it as the one that cannot be deleted.
pub const DEFAULT_GROUP: &str = "No group";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels))
        .route("/labels/create", post(create_label))
        .route("/labels/{id}/panel", get(label_panel))
        .route("/labels/{id}/delete", post(delete_label))
        .route("/labels/{id}/edit", post(edit_label))
        .route("/labels/{id}/group", post(move_label))
        .route("/labels/{id}/rules", get(list_rules))
        .route("/labels/{id}/rules/create", post(create_rule))
        .route(
            "/labels/{label_id}/rules/{rule_id}/delete",
            post(delete_rule),
        )
        .route("/label-groups/create", post(create_group))
        .route("/label-groups/{id}/panel", get(group_panel))
        .route("/label-groups/{id}/edit", post(edit_group))
        .route("/label-groups/{id}/delete", post(delete_group))
}

// ── Groups ───────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub struct GroupRow {
    pub id: i64,
    pub name: String,
    pub is_default: bool,
}

/// The seeded default name is translated; anything the user typed is shown as
/// they typed it. Returns RAW text — HTML callers must escape it.
pub fn group_display_name(name: &str, lang: Lang) -> String {
    if name == DEFAULT_GROUP {
        super::i18n::t(lang).lbl_group_other.to_string()
    } else {
        name.to_string()
    }
}

/// Id of the user's default group, creating it if this user has never had one.
pub async fn ensure_other_group(pool: &sqlx::SqlitePool, user_id: i64) -> Result<i64, sqlx::Error> {
    if let Some((id,)) = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM leanfin_label_groups WHERE user_id = ? AND is_default = 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    // ON CONFLICT covers the case where a user already made a group under this
    // name by hand: promote it rather than failing the unique index.
    sqlx::query_scalar(
        "INSERT INTO leanfin_label_groups (user_id, name, is_default) VALUES (?, ?, 1)
         ON CONFLICT(user_id, name) DO UPDATE SET is_default = 1
         RETURNING id",
    )
    .bind(user_id)
    .bind(DEFAULT_GROUP)
    .fetch_one(pool)
    .await
}

/// Every group the user has, default first and the rest alphabetically. Also
/// repairs labels whose group went missing, so `group_id` is never NULL in the
/// rows the page goes on to render.
pub async fn user_groups(
    pool: &sqlx::SqlitePool,
    user_id: i64,
) -> Result<Vec<GroupRow>, sqlx::Error> {
    let other_id = ensure_other_group(pool, user_id).await?;

    sqlx::query("UPDATE leanfin_labels SET group_id = ? WHERE user_id = ? AND group_id IS NULL")
        .bind(other_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    sqlx::query_as::<_, GroupRow>(
        "SELECT id, name, is_default FROM leanfin_label_groups WHERE user_id = ?
         ORDER BY is_default DESC, name COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// `<option>` list of the user's groups, with `selected` on `selected_id`.
fn group_options(groups: &[GroupRow], selected_id: Option<i64>, lang: Lang) -> String {
    let mut out = String::new();
    for g in groups {
        out.push_str(&format!(
            r#"<option value="{id}"{selected}>{name}</option>"#,
            id = g.id,
            selected = if selected_id == Some(g.id) {
                " selected"
            } else {
                ""
            },
            name = html_escape(&group_display_name(&g.name, lang)),
        ));
    }
    out
}

// ── List ─────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct LabelRow {
    id: i64,
    name: String,
    group_id: Option<i64>,
}

async fn list_labels(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let groups = user_groups(&state.pool, user_id.0)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        });

    let labels: Vec<LabelRow> = sqlx::query_as(
        "SELECT id, name, group_id FROM leanfin_labels WHERE user_id = ? ORDER BY name COLLATE NOCASE",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    // Each group is a heading plus a flow of label chips. Details for either
    // open in the one slot at the end of the group, which is what keeps a
    // single frame open at a time.
    let mut sections = String::new();
    for g in &groups {
        let color = group_color(g.id);
        let members: Vec<&LabelRow> = labels.iter().filter(|l| l.group_id == Some(g.id)).collect();

        let mut chips = String::new();
        for l in &members {
            chips.push_str(&format!(
                r#"<button type="button" class="lf-chip" data-url="{base}/leanfin/labels/{id}/panel"
                        onclick="lfPanel(this)"><span class="label-badge">{name}</span></button>"#,
                id = l.id,
                name = html_escape(&l.name),
            ));
        }
        if chips.is_empty() {
            chips = format!(
                r#"<span class="text-secondary text-sm lf-group-empty">{}</span>"#,
                t.lbl_group_empty
            );
        }

        sections.push_str(&format!(
            r#"<div class="lf-group" style="--label-color:{color}">
                <button type="button" class="lf-group-head" data-url="{base}/leanfin/label-groups/{id}/panel"
                        onclick="lfPanel(this)">
                    <span class="label-badge">{name}</span>
                    <span class="lf-count text-secondary text-sm">{count}</span>
                </button>
                <div class="lf-chips">{chips}</div>
                <div class="lf-detail"></div>
            </div>"#,
            id = g.id,
            name = html_escape(&group_display_name(&g.name, lang)),
            count = members.len(),
        ));
    }

    if labels.is_empty() {
        sections.push_str(&format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.lbl_no_labels
        ));
    }

    let create_options = group_options(&groups, groups.first().map(|g| g.id), lang);

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="card" style="max-width:42rem;">
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
                            <label for="group_id">{lbl_group}</label>
                            <select id="group_id" name="group_id">{create_options}</select>
                        </div>
                    </div>
                    <button type="submit">{create_btn}</button>
                </form>
            </div>
        </div>

        <div class="card mt-2" style="max-width:42rem;">
            <div class="card-header">
                <h2>{create_group}</h2>
            </div>
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/label-groups/create" class="label-create-form">
                    <div class="form-row">
                        <div class="form-group" style="flex:1">
                            <label for="group-name">{group_name}</label>
                            <input type="text" id="group-name" name="name" required placeholder="{group_placeholder}">
                        </div>
                    </div>
                    <button type="submit">{create_group_btn}</button>
                </form>
            </div>
        </div>

        <div class="card mt-2" style="max-width:42rem;">
            <div class="card-header">
                <h2>{groups_heading}</h2>
            </div>
            <div class="card-body">
                {sections}
            </div>
        </div>

        <script>
        // Only one frame is ever open: opening any panel clears every other
        // slot first, and clicking the open trigger again closes it.
        window.lfPanel = function(btn) {{
            var slot = btn.closest('.lf-group').querySelector('.lf-detail');
            var wasOpen = btn.classList.contains('lf-open');
            document.querySelectorAll('.lf-detail').forEach(function(d) {{ d.innerHTML = ''; }});
            document.querySelectorAll('.lf-open').forEach(function(b) {{ b.classList.remove('lf-open'); }});
            if (wasOpen) return;
            btn.classList.add('lf-open');
            htmx.ajax('GET', btn.dataset.url, slot);
        }};
        </script>"##,
        title = t.lbl_title,
        subtitle = t.lbl_subtitle,
        create = t.lbl_create,
        create_group = t.lbl_group_create,
        create_group_btn = t.lbl_group_create_btn,
        groups_heading = t.lbl_groups,
        group_name = t.lbl_group_name,
        group_placeholder = t.lbl_group_placeholder,
        lbl_name = t.lbl_name,
        lbl_group = t.lbl_group,
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

// ── Group panel ──────────────────────────────────────────────

async fn group_panel(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let group: Option<GroupRow> = sqlx::query_as(
        "SELECT id, name, is_default FROM leanfin_label_groups WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(group) = group else {
        return Html(String::new());
    };

    // The default group is where labels land when a group is deleted, so it has
    // nowhere to fall back to and stays.
    let delete_block = if group.is_default {
        format!(
            r#"<p class="text-secondary text-sm">{}</p>"#,
            t.lbl_group_default_hint
        )
    } else {
        format!(
            r#"<form method="POST" action="{base}/leanfin/label-groups/{id}/delete"
                      onsubmit="return confirm('{confirm}')">
                <button type="submit" class="btn btn-danger btn-sm">{delete}</button>
            </form>"#,
            confirm = t.lbl_group_delete_confirm,
            delete = t.lbl_group_delete,
        )
    };

    Html(format!(
        r#"<div class="lf-panel">
            <form method="POST" action="{base}/leanfin/label-groups/{id}/edit" class="lf-panel-row">
                <input type="text" name="name" value="{name}" required aria-label="{group_name}">
                <button type="submit" class="btn btn-primary btn-sm">{save}</button>
            </form>
            <div class="lf-panel-row">{delete_block}</div>
        </div>"#,
        name = html_escape(&group_display_name(&group.name, lang)),
        group_name = t.lbl_group_name,
        save = t.lbl_save,
    ))
}

// ── Label panel ──────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct LabelDetail {
    id: i64,
    name: String,
    group_id: Option<i64>,
    rule_count: i32,
    txn_count: i32,
}

async fn label_panel(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let label: Option<LabelDetail> = sqlx::query_as(
        r#"SELECT l.id, l.name, l.group_id,
                  (SELECT COUNT(*) FROM leanfin_label_rules WHERE label_id = l.id) AS rule_count,
                  (SELECT COUNT(*) FROM leanfin_allocations WHERE label_id = l.id) AS txn_count
           FROM leanfin_labels l WHERE l.id = ? AND l.user_id = ?"#,
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(label) = label else {
        return Html(String::new());
    };

    let groups = user_groups(&state.pool, user_id.0)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        });

    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT id, field, pattern, priority FROM leanfin_label_rules WHERE label_id = ? ORDER BY priority DESC, id",
    )
    .bind(label.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    Html(format!(
        r#"<div class="lf-panel">
            <form method="POST" action="{base}/leanfin/labels/{id}/edit" class="lf-panel-row">
                <input type="text" name="name" value="{name}" required aria-label="{lbl_name}">
                <button type="submit" class="btn btn-primary btn-sm">{save}</button>
            </form>
            <form method="POST" action="{base}/leanfin/labels/{id}/group" class="lf-panel-row">
                <select name="group_id" aria-label="{lbl_group}">{group_options}</select>
                <button type="submit" class="btn btn-secondary btn-sm">{move_btn}</button>
            </form>
            <div id="rules-{id}">{rules_panel}</div>
            <div class="lf-panel-row lf-panel-footer">
                <span class="text-secondary text-sm">{rule_count}r / {txn_count}t</span>
                <form method="POST" action="{base}/leanfin/labels/{id}/delete"
                      onsubmit="return confirm('{delete_confirm}')">
                    <button type="submit" class="btn btn-danger btn-sm">{delete}</button>
                </form>
            </div>
        </div>"#,
        id = label.id,
        name = html_escape(&label.name),
        lbl_name = t.lbl_name,
        lbl_group = t.lbl_group,
        save = t.lbl_save,
        move_btn = t.lbl_group_move,
        group_options = group_options(&groups, label.group_id, lang),
        rules_panel = render_rules_panel(base, label.id, &rules, lang),
        rule_count = label.rule_count,
        txn_count = label.txn_count,
        delete = t.lbl_delete,
        delete_confirm = t.lbl_delete_confirm,
    ))
}

// ── Create / edit / delete groups ────────────────────────────

#[derive(Deserialize)]
struct GroupNameForm {
    name: String,
}

async fn create_group(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<GroupNameForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let name = form.name.trim();

    if !name.is_empty()
        && let Err(e) =
            sqlx::query("INSERT OR IGNORE INTO leanfin_label_groups (user_id, name) VALUES (?, ?)")
                .bind(user_id.0)
                .bind(name)
                .execute(&state.pool)
                .await
    {
        tracing::error!("Failed to create label group: {e}");
    }
    Redirect::to(&format!("{base}/leanfin/labels"))
}

async fn edit_group(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<GroupNameForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let name = form.name.trim();

    // A clash with another group's name is a unique-index error, not a crash.
    if !name.is_empty()
        && let Err(e) =
            sqlx::query("UPDATE leanfin_label_groups SET name = ? WHERE id = ? AND user_id = ?")
                .bind(name)
                .bind(id)
                .bind(user_id.0)
                .execute(&state.pool)
                .await
    {
        tracing::warn!("Failed to rename label group {id}: {e}");
    }
    Redirect::to(&format!("{base}/leanfin/labels"))
}

async fn delete_group(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Labels outlive their group: move them home before the row disappears.
    if let Ok(other_id) = ensure_other_group(&state.pool, user_id.0).await
        && other_id != id
    {
        sqlx::query("UPDATE leanfin_labels SET group_id = ? WHERE group_id = ? AND user_id = ?")
            .bind(other_id)
            .bind(id)
            .bind(user_id.0)
            .execute(&state.pool)
            .await
            .ok();

        sqlx::query(
            "DELETE FROM leanfin_label_groups WHERE id = ? AND user_id = ? AND is_default = 0",
        )
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    }

    Redirect::to(&format!("{base}/leanfin/labels"))
}

// ── Create label ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateLabelForm {
    name: String,
    group_id: Option<i64>,
}

async fn create_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<CreateLabelForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let group_id = resolve_group(&state.pool, user_id.0, form.group_id).await;

    if let Err(e) =
        sqlx::query("INSERT INTO leanfin_labels (user_id, name, group_id) VALUES (?, ?, ?)")
            .bind(user_id.0)
            .bind(&form.name)
            .bind(group_id)
            .execute(&state.pool)
            .await
    {
        tracing::error!("Failed to create label: {e}");
    }
    Redirect::to(&format!("{base}/leanfin/labels"))
}

/// Validate that a submitted group belongs to the user, falling back to the
/// default group. Returns `None` only if the default group cannot be created.
async fn resolve_group(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    requested: Option<i64>,
) -> Option<i64> {
    if let Some(id) = requested {
        let owns: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM leanfin_label_groups WHERE id = ? AND user_id = ?)",
        )
        .bind(id)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
        if owns {
            return Some(id);
        }
    }
    ensure_other_group(pool, user_id).await.ok()
}

// ── Edit / move / delete label ───────────────────────────────

#[derive(Deserialize)]
struct EditLabelForm {
    name: String,
}

async fn edit_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<EditLabelForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    sqlx::query("UPDATE leanfin_labels SET name = ? WHERE id = ? AND user_id = ?")
        .bind(&form.name)
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();
    Redirect::to(&format!("{base}/leanfin/labels"))
}

#[derive(Deserialize)]
struct MoveLabelForm {
    group_id: i64,
}

async fn move_label(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<MoveLabelForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let group_id = resolve_group(&state.pool, user_id.0, Some(form.group_id)).await;

    sqlx::query("UPDATE leanfin_labels SET group_id = ? WHERE id = ? AND user_id = ?")
        .bind(group_id)
        .bind(id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();

    Redirect::to(&format!("{base}/leanfin/labels"))
}

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
            field = html_escape(&r.field),
            pattern = html_escape(&r.pattern),
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
            r##"<p class="text-secondary text-sm rules-panel-hint">{hint}</p>"##,
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
        hint = t.lbl_rules_hint,
        counterparty = t.lbl_counterparty,
        description = t.lbl_description,
        contains = t.lbl_contains,
        priority = t.lbl_priority,
        add_rule = t.lbl_add_rule,
    )
}

async fn owns_label(pool: &sqlx::SqlitePool, label_id: i64, user_id: i64) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM leanfin_labels WHERE id = ? AND user_id = ?")
        .bind(label_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
        > 0
}

async fn fetch_rules(pool: &sqlx::SqlitePool, label_id: i64) -> Vec<RuleRow> {
    sqlx::query_as(
        "SELECT id, field, pattern, priority FROM leanfin_label_rules WHERE label_id = ? ORDER BY priority DESC, id",
    )
    .bind(label_id)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    })
}

async fn list_rules(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(label_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;
    if !owns_label(&state.pool, label_id, user_id.0).await {
        return Html(String::new());
    }
    let rules = fetch_rules(&state.pool, label_id).await;
    Html(render_rules_panel(base, label_id, &rules, lang))
}

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

    if !owns_label(&state.pool, label_id, user_id.0).await {
        return Html(String::new());
    }
    if form.field != "description" && form.field != "counterparty" {
        return Html(String::new());
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO leanfin_label_rules (label_id, field, pattern, priority) VALUES (?, ?, ?, ?)",
    )
    .bind(label_id)
    .bind(&form.field)
    .bind(&form.pattern)
    .bind(form.priority.unwrap_or(0))
    .execute(&state.pool)
    .await
    {
        tracing::error!("Failed to create rule: {e}");
    }

    let rules = fetch_rules(&state.pool, label_id).await;
    Html(render_rules_panel(base, label_id, &rules, lang))
}

async fn delete_rule(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path((label_id, rule_id)): Path<(i64, i64)>,
) -> Html<String> {
    let base = &state.config.base_path;

    // The DELETE below is already scoped to the caller, but the panel this
    // handler re-renders afterwards is not: rendering it for a label someone
    // else owns would hand back their rule patterns.
    if !owns_label(&state.pool, label_id, user_id.0).await {
        return Html(String::new());
    }

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

    let rules = fetch_rules(&state.pool, label_id).await;
    Html(render_rules_panel(base, label_id, &rules, lang))
}

use axum::{
    Extension, Form, Router,
    extract::{Path, Query},
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;

use super::models::Transaction;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transactions", get(list))
        .route("/transactions/{txn_id}/allocations", get(alloc_editor))
        .route("/transactions/{txn_id}/allocations/add", post(alloc_add))
        .route(
            "/transactions/{txn_id}/allocations/{alloc_id}/delete",
            post(alloc_delete),
        )
        .route("/transactions/{txn_id}/row", get(txn_row))
        .route("/transactions/{txn_id}/rules/create", post(rule_create))
}

// ── Shared types ─────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct AllocRow {
    id: i64,
    transaction_id: i64,
    label_id: i64,
    amount: f64,
    label_name: String,
    label_color: Option<String>,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct LabelInfo {
    id: i64,
    name: String,
    color: Option<String>,
}

// ── Render helpers ───────────────────────────────────────────

fn render_badges(allocs: &[&AllocRow], base: &str, txn_id: i64) -> String {
    let mut html = String::new();
    for a in allocs {
        let color = a.label_color.as_deref().unwrap_or("#6B6B6B");
        let display_amount = if allocs.len() > 1 {
            format!(" {:.2}", a.amount)
        } else {
            String::new()
        };
        html.push_str(&format!(
            r#"<span class="label-badge label-badge-sm" style="--label-color:{color}">{}{display_amount}</span> "#,
            a.label_name,
        ));
    }
    html.push_str(&format!(
        r##"<span class="label-add-btn"
                hx-get="{base}/leanfin/transactions/{txn_id}/allocations"
                hx-target="closest tr"
                hx-swap="afterend"
                hx-on::before-request="var e=document.getElementById('alloc-editor-{txn_id}');if(e){{e.remove();}}"
                hx-on::after-request="this.closest('tr').querySelector('.txn-labels').classList.add('editing')">+</span>"##
    ));
    html
}

fn render_row(t: &Transaction, txn_allocs: &[&AllocRow], base: &str) -> String {
    let counterparty = t.counterparty.as_deref().unwrap_or("—");
    let sign = if t.amount < 0.0 {
        "negative"
    } else {
        "positive"
    };
    let balance = t
        .balance_after
        .map_or("—".to_string(), |b| format!("{b:.2}"));
    let badge_html = render_badges(txn_allocs, base, t.id);

    let allocated: f64 = txn_allocs.iter().map(|a| a.amount).sum();
    let abs_total = t.amount.abs();
    let alloc_class = if txn_allocs.is_empty() {
        "txn-unallocated"
    } else if (allocated - abs_total).abs() < 0.01 {
        ""
    } else {
        "txn-misallocated"
    };

    format!(
        r#"<tr id="txn-{id}" class="{alloc_class}">
            <td class="txn-date">{date}</td>
            <td class="txn-counterparty">{counterparty}</td>
            <td class="txn-description">{desc}</td>
            <td class="txn-labels">{badge_html}</td>
            <td class="txn-amount {sign}">{amount:+.2} {currency}</td>
            <td class="txn-balance">{balance}</td>
        </tr>"#,
        id = t.id,
        date = t.date,
        desc = t.description,
        amount = t.amount,
        currency = t.currency,
    )
}

// ── Transaction list (HTMX partial) ─────────────────────────

const PAGE_SIZE: i64 = 50;

#[derive(Deserialize, Default)]
struct FilterParams {
    q: Option<String>,
    account_id: Option<String>,
    unallocated: Option<String>,
    label_ids: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    page: Option<i64>,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Query(filter): Query<FilterParams>,
) -> Html<String> {
    let t = super::i18n::t(lang);
    let base = &state.config.base_path;
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;

    // Build dynamic WHERE clause (shared between count and data queries)
    let mut where_clause = String::from(" WHERE a.user_id = ?");

    let account_id: Option<i64> = filter.account_id.as_deref().and_then(|s| s.parse().ok());
    if account_id.is_some() {
        where_clause.push_str(" AND t.account_id = ?");
    }

    let q = filter.q.as_deref().unwrap_or("").trim().to_string();
    if !q.is_empty() {
        where_clause.push_str(" AND (t.description LIKE ? OR t.counterparty LIKE ?)");
    }

    let show_unallocated = filter.unallocated.is_some();
    if show_unallocated {
        where_clause.push_str(
            r#" AND t.id NOT IN (
                SELECT al.transaction_id FROM leanfin_allocations al
                GROUP BY al.transaction_id
                HAVING ABS(SUM(al.amount) - ABS(
                    (SELECT t2.amount FROM leanfin_transactions t2 WHERE t2.id = al.transaction_id)
                )) < 0.01
            )"#,
        );
    }

    // Filter by label IDs (comma-separated)
    let label_ids: Vec<i64> = filter
        .label_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();
    if !label_ids.is_empty() {
        let placeholders = label_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        where_clause.push_str(&format!(
            " AND t.id IN (SELECT transaction_id FROM leanfin_allocations WHERE label_id IN ({placeholders}))"
        ));
    }

    let date_from = filter.date_from.as_deref().unwrap_or("").trim().to_string();
    if !date_from.is_empty() {
        where_clause.push_str(" AND t.date >= ?");
    }

    let date_to = filter.date_to.as_deref().unwrap_or("").trim().to_string();
    if !date_to.is_empty() {
        where_clause.push_str(" AND t.date <= ?");
    }

    // Helper to bind filter params to a query
    macro_rules! bind_filters {
        ($query:expr) => {{
            let mut q_bound = $query.bind(user_id.0);
            if let Some(aid) = account_id {
                q_bound = q_bound.bind(aid);
            }
            if !q.is_empty() {
                let pattern = format!("%{q}%");
                q_bound = q_bound.bind(pattern.clone());
                q_bound = q_bound.bind(pattern);
            }
            for lid in &label_ids {
                q_bound = q_bound.bind(*lid);
            }
            if !date_from.is_empty() {
                q_bound = q_bound.bind(date_from.clone());
            }
            if !date_to.is_empty() {
                q_bound = q_bound.bind(date_to.clone());
            }
            q_bound
        }};
    }

    // Count total matching transactions
    let count_sql = format!(
        "SELECT COUNT(*) as cnt FROM leanfin_transactions t JOIN leanfin_accounts a ON t.account_id = a.id{where_clause}"
    );
    let total: i64 = bind_filters!(sqlx::query_scalar::<_, i64>(&count_sql))
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    if total == 0 {
        return Html(format!(
            r#"<div class="empty-state">
            <p>{}</p>
        </div>"#,
            t.txn_no_transactions
        ));
    }

    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
    let page = page.min(total_pages);

    // Fetch page of transactions
    let data_sql = format!(
        r#"SELECT t.id, t.account_id, t.external_id, t.date, t.amount,
               t.currency, t.description, t.counterparty, t.balance_after,
               t.created_at, t.snapshot_id
        FROM leanfin_transactions t
        JOIN leanfin_accounts a ON t.account_id = a.id{where_clause}
        ORDER BY t.date DESC LIMIT ? OFFSET ?"#
    );
    let transactions: Vec<Transaction> = bind_filters!(sqlx::query_as::<_, Transaction>(&data_sql))
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    // Fetch all allocations for these transactions
    let txn_ids: Vec<i64> = transactions.iter().map(|t| t.id).collect();
    let placeholders = txn_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query_str = format!(
        r#"SELECT a.id, a.transaction_id, a.label_id, a.amount,
                  l.name AS label_name, l.color AS label_color
           FROM leanfin_allocations a
           JOIN leanfin_labels l ON a.label_id = l.id
           WHERE a.transaction_id IN ({placeholders})
           ORDER BY a.amount DESC"#
    );
    let mut alloc_query = sqlx::query_as::<_, AllocRow>(&query_str);
    for id in &txn_ids {
        alloc_query = alloc_query.bind(id);
    }
    let allocs: Vec<AllocRow> = alloc_query.fetch_all(&state.pool).await.unwrap_or_default();

    let mut rows = String::new();
    for t in &transactions {
        let txn_allocs: Vec<&AllocRow> =
            allocs.iter().filter(|a| a.transaction_id == t.id).collect();
        rows.push_str(&render_row(t, &txn_allocs, base));
    }

    // Pagination controls
    let from_row = offset + 1;
    let to_row = (offset + PAGE_SIZE).min(total);
    let mut pagination = format!(
        r#"<div class="pagination">
            <span class="pagination-info">{from_row}–{to_row} of {total}</span>
            <div class="pagination-buttons">"#
    );
    if page > 1 {
        pagination.push_str(&format!(
            r##"<button class="btn btn-secondary btn-sm"
                    hx-get="{base}/leanfin/transactions"
                    hx-target="#txn-table"
                    hx-include="#txn-filters"
                    hx-vals='{{"page":{prev}}}'>{}</button>"##,
            t.txn_prev,
            prev = page - 1,
        ));
    }
    if page < total_pages {
        pagination.push_str(&format!(
            r##"<button class="btn btn-secondary btn-sm"
                    hx-get="{base}/leanfin/transactions"
                    hx-target="#txn-table"
                    hx-include="#txn-filters"
                    hx-vals='{{"page":{next}}}'>{}</button>"##,
            t.txn_next,
            next = page + 1,
        ));
    }
    pagination.push_str("</div></div>");

    Html(format!(
        r#"<table>
            <thead><tr>
                <th>{col_date}</th>
                <th>{col_counterparty}</th>
                <th>{col_description}</th>
                <th>{col_labels}</th>
                <th>{col_amount}</th>
                <th>{col_balance}</th>
            </tr></thead>
            <tbody>{rows}</tbody>
        </table>
        {pagination}"#,
        col_date = t.txn_col_date,
        col_counterparty = t.txn_col_counterparty,
        col_description = t.txn_col_description,
        col_labels = t.txn_col_labels,
        col_amount = t.txn_col_amount,
        col_balance = t.txn_col_balance,
    ))
}

// ── Allocation editor (HTMX partial, inserted as row below) ─

async fn alloc_editor(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(txn_id): Path<i64>,
) -> Html<String> {
    alloc_editor_inner(&state, user_id, lang, txn_id, None).await
}

async fn alloc_editor_inner(
    state: &axum::extract::State<AppState>,
    user_id: UserId,
    lang: Lang,
    txn_id: i64,
    flash: Option<&str>,
) -> Html<String> {
    let t = super::i18n::t(lang);
    let base = &state.config.base_path;

    // Get transaction details (amount, counterparty, description)
    let txn: Option<(f64, String, Option<String>)> = sqlx::query_as(
        r#"SELECT t.amount, t.description, t.counterparty FROM leanfin_transactions t
           JOIN leanfin_accounts a ON t.account_id = a.id
           WHERE t.id = ? AND a.user_id = ?"#,
    )
    .bind(txn_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((txn_amount, txn_description, txn_counterparty)) = txn else {
        return Html("".to_string());
    };

    let abs_total = txn_amount.abs();

    // Current allocations
    let allocs: Vec<AllocRow> = sqlx::query_as(
        r#"SELECT a.id, a.transaction_id, a.label_id, a.amount,
                  l.name AS label_name, l.color AS label_color
           FROM leanfin_allocations a
           JOIN leanfin_labels l ON a.label_id = l.id
           WHERE a.transaction_id = ?
           ORDER BY a.amount DESC"#,
    )
    .bind(txn_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // All labels for picker
    let labels: Vec<LabelInfo> = sqlx::query_as(
        "SELECT id, name, color FROM leanfin_labels WHERE user_id = ? ORDER BY name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let allocated: f64 = allocs.iter().map(|a| a.amount).sum();
    let remaining = abs_total - allocated;

    // Render existing allocations
    let mut alloc_rows = String::new();
    for a in &allocs {
        let color = a.label_color.as_deref().unwrap_or("#6B6B6B");
        alloc_rows.push_str(&format!(
            r##"<div class="alloc-row">
                <span class="label-badge" style="--label-color:{color}">{name}</span>
                <span class="alloc-amount mono">{amount:.2}</span>
                <form method="POST"
                      hx-post="{base}/leanfin/transactions/{txn_id}/allocations/{alloc_id}/delete"
                      hx-target="#alloc-editor-{txn_id}"
                      hx-swap="outerHTML">
                    <button class="btn-icon btn-icon-danger" type="submit">&times;</button>
                </form>
            </div>"##,
            name = a.label_name,
            amount = a.amount,
            alloc_id = a.id,
        ));
    }

    // Label picker options
    let mut options = format!(
        r#"<option value="" disabled selected>{}</option>"#,
        t.alloc_choose_label
    );
    for l in &labels {
        // Skip labels already allocated
        let already = allocs.iter().any(|a| a.label_id == l.id);
        if already {
            continue;
        }
        options.push_str(&format!(r#"<option value="{}">{}</option>"#, l.id, l.name,));
    }

    let remaining_class = if remaining.abs() < 0.01 {
        "alloc-remaining-ok"
    } else {
        "alloc-remaining-pending"
    };

    // Build "Add Rule" form with pre-filled values
    let counterparty_val = txn_counterparty.as_deref().unwrap_or("");
    let description_val = &txn_description;
    let default_field;
    let default_pattern;
    if !counterparty_val.is_empty() {
        default_field = "counterparty";
        default_pattern = counterparty_val;
    } else {
        default_field = "description";
        default_pattern = &txn_description;
    }

    let flash_html = if let Some(msg) = flash {
        format!(r#"<div class="alloc-flash">{msg}</div>"#)
    } else {
        String::new()
    };

    Html(format!(
        r##"<tr id="alloc-editor-{txn_id}" class="alloc-editor-row">
            <td colspan="6">
                <div class="alloc-editor">
                    <div class="alloc-header">
                        <span class="text-sm"><strong>{alloc_title}</strong> — total: <span class="mono">{abs_total:.2}</span></span>
                        <span class="text-sm {remaining_class}">{alloc_remaining}: <span class="mono">{remaining:.2}</span></span>
                    </div>
                    {flash_html}
                    <div class="alloc-list">{alloc_rows}</div>
                    <form class="alloc-add-form"
                          hx-post="{base}/leanfin/transactions/{txn_id}/allocations/add"
                          hx-target="#alloc-editor-{txn_id}"
                          hx-swap="outerHTML">
                        <select name="label_id" required>{options}</select>
                        <input type="number" name="amount" step="0.01" min="0.01"
                               value="{remaining:.2}" placeholder="{alloc_amount}" required
                               class="alloc-amount-input mono">
                        <button type="submit" class="btn btn-primary btn-sm">{alloc_add}</button>
                    </form>
                    <div class="alloc-rule-form">
                        <span class="text-sm" style="font-weight:600;text-transform:uppercase;letter-spacing:0.04em;color:var(--text-secondary)">{alloc_add_rule}</span>
                        <form class="rule-add-form"
                              hx-post="{base}/leanfin/transactions/{txn_id}/rules/create"
                              hx-target="#alloc-editor-{txn_id}"
                              hx-swap="outerHTML">
                            <select name="label_id" required>{rule_label_options}</select>
                            <select name="field" required
                                    data-cp="{counterparty_val}"
                                    data-desc="{description_val}"
                                    onchange="this.closest('form').querySelector('input[name=pattern]').value=this.selectedOptions[0].value==='counterparty'?this.dataset.cp:this.dataset.desc">
                                <option value="counterparty" {sel_cp}>{lbl_counterparty}</option>
                                <option value="description" {sel_desc}>{lbl_description}</option>
                            </select>
                            <input type="text" name="pattern" value="{default_pattern}" placeholder="{alloc_rule_pattern}" required style="flex:1">
                            <button type="submit" class="btn btn-primary btn-sm">{alloc_add_rule}</button>
                        </form>
                    </div>
                    <div class="alloc-footer">
                        <button class="btn btn-secondary btn-sm"
                                hx-get="{base}/leanfin/transactions/{txn_id}/row"
                                hx-target="#txn-{txn_id}"
                                hx-swap="outerHTML"
                                onclick="var e=document.getElementById('alloc-editor-{txn_id}');if(e)setTimeout(function(){{e.remove()}},100)">{alloc_done}</button>
                    </div>
                </div>
            </td>
        </tr>"##,
        alloc_title = t.alloc_title,
        alloc_remaining = t.alloc_remaining,
        alloc_amount = t.alloc_amount,
        alloc_add = t.alloc_add,
        alloc_done = t.alloc_done,
        alloc_add_rule = t.alloc_add_rule,
        alloc_rule_pattern = t.alloc_rule_pattern,
        lbl_counterparty = t.lbl_counterparty,
        lbl_description = t.lbl_description,
        sel_cp = if default_field == "counterparty" {
            "selected"
        } else {
            ""
        },
        sel_desc = if default_field == "description" {
            "selected"
        } else {
            ""
        },
        counterparty_val = counterparty_val,
        description_val = description_val,
        rule_label_options = options,
    ))
}

// ── Add allocation ───────────────────────────────────────────

#[derive(Deserialize)]
struct AddAllocForm {
    label_id: i64,
    amount: f64,
}

async fn alloc_add(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(txn_id): Path<i64>,
    Form(form): Form<AddAllocForm>,
) -> Html<String> {
    // Verify label belongs to user
    let owns: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE id = ? AND user_id = ?")
            .bind(form.label_id)
            .bind(user_id.0)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if owns.is_some() {
        sqlx::query(
            "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
        )
        .bind(txn_id)
        .bind(form.label_id)
        .bind(form.amount)
        .execute(&state.pool)
        .await
        .ok();
    }

    alloc_editor_inner(&state, user_id, lang, txn_id, None).await
}

// ── Delete allocation ────────────────────────────────────────

async fn alloc_delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path((txn_id, alloc_id)): Path<(i64, i64)>,
) -> Html<String> {
    // Delete only if the allocation's label belongs to the user
    sqlx::query(
        r#"DELETE FROM leanfin_allocations WHERE id = ? AND transaction_id = ?
           AND label_id IN (SELECT id FROM leanfin_labels WHERE user_id = ?)"#,
    )
    .bind(alloc_id)
    .bind(txn_id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    alloc_editor_inner(&state, user_id, lang, txn_id, None).await
}

// ── Create rule from transaction ─────────────────────────────

#[derive(Deserialize)]
struct CreateRuleForm {
    label_id: i64,
    field: String,
    pattern: String,
}

async fn rule_create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(txn_id): Path<i64>,
    Form(form): Form<CreateRuleForm>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    // Validate field
    if form.field != "description" && form.field != "counterparty" {
        return alloc_editor_inner(&state, user_id, lang, txn_id, None).await;
    }

    // Verify label belongs to user
    let owns: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE id = ? AND user_id = ?")
            .bind(form.label_id)
            .bind(user_id.0)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if owns.is_none() {
        return alloc_editor_inner(&state, user_id, lang, txn_id, None).await;
    }

    // Create the rule
    if let Err(e) = sqlx::query(
        "INSERT INTO leanfin_label_rules (label_id, field, pattern, priority) VALUES (?, ?, ?, 0)",
    )
    .bind(form.label_id)
    .bind(&form.field)
    .bind(&form.pattern)
    .execute(&state.pool)
    .await
    {
        tracing::error!("Failed to create rule from transaction: {e}");
        return alloc_editor_inner(&state, user_id, lang, txn_id, None).await;
    }

    // Apply rules to all unallocated transactions
    match super::services::labeling::apply_rules(&state.pool, user_id.0).await {
        Ok(n) => tracing::info!("Rule created from txn {txn_id}: auto-labeled {n} transactions"),
        Err(e) => tracing::error!("Failed to apply rules after creation: {e}"),
    }

    alloc_editor_inner(&state, user_id, lang, txn_id, Some(t.alloc_rule_created)).await
}

// ── Single row (for "Done" button — refreshes row with correct class) ──

async fn txn_row(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(txn_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

    let txn: Option<Transaction> = sqlx::query_as(
        r#"SELECT t.id, t.account_id, t.external_id, t.date, t.amount,
               t.currency, t.description, t.counterparty, t.balance_after,
               t.created_at, t.snapshot_id
        FROM leanfin_transactions t
        JOIN leanfin_accounts a ON t.account_id = a.id
        WHERE t.id = ? AND a.user_id = ?"#,
    )
    .bind(txn_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(t) = txn else {
        return Html("".to_string());
    };

    let allocs: Vec<AllocRow> = sqlx::query_as(
        r#"SELECT a.id, a.transaction_id, a.label_id, a.amount,
                  l.name AS label_name, l.color AS label_color
           FROM leanfin_allocations a
           JOIN leanfin_labels l ON a.label_id = l.id
           WHERE a.transaction_id = ?
           ORDER BY a.amount DESC"#,
    )
    .bind(txn_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let refs: Vec<&AllocRow> = allocs.iter().collect();
    Html(render_row(&t, &refs, base))
}

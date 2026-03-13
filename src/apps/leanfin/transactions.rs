use axum::{
    Extension, Form, Router,
    extract::{Path, Query},
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;

use crate::routes::AppState;
use crate::auth::UserId;
use crate::models::Transaction;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transactions", get(list))
        .route("/transactions/{txn_id}/allocations", get(alloc_editor))
        .route("/transactions/{txn_id}/allocations/add", post(alloc_add))
        .route("/transactions/{txn_id}/allocations/{alloc_id}/delete", post(alloc_delete))
        .route("/transactions/{txn_id}/row", get(txn_row))
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
    let sign = if t.amount < 0.0 { "negative" } else { "positive" };
    let balance = t.balance_after.map_or("—".to_string(), |b| format!("{b:.2}"));
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

#[derive(Deserialize, Default)]
struct FilterParams {
    q: Option<String>,
    account_id: Option<String>,
    unallocated: Option<String>,
    label_ids: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Query(filter): Query<FilterParams>,
) -> Html<String> {
    let base = &state.config.base_path;

    // Build dynamic query based on filters
    let mut sql = String::from(
        r#"SELECT t.id, t.account_id, t.external_id, t.date, t.amount,
               t.currency, t.description, t.counterparty, t.balance_after,
               t.created_at
        FROM transactions t
        JOIN accounts a ON t.account_id = a.id
        WHERE a.user_id = ?"#,
    );

    let account_id: Option<i64> = filter.account_id.as_deref().and_then(|s| s.parse().ok());
    if account_id.is_some() {
        sql.push_str(" AND t.account_id = ?");
    }

    let q = filter.q.as_deref().unwrap_or("").trim().to_string();
    if !q.is_empty() {
        sql.push_str(" AND (t.description LIKE ? OR t.counterparty LIKE ?)");
    }

    let show_unallocated = filter.unallocated.is_some();
    if show_unallocated {
        sql.push_str(
            r#" AND t.id NOT IN (
                SELECT al.transaction_id FROM allocations al
                GROUP BY al.transaction_id
                HAVING ABS(SUM(al.amount) - ABS(
                    (SELECT t2.amount FROM transactions t2 WHERE t2.id = al.transaction_id)
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
        sql.push_str(&format!(
            " AND t.id IN (SELECT transaction_id FROM allocations WHERE label_id IN ({placeholders}))"
        ));
    }

    let date_from = filter.date_from.as_deref().unwrap_or("").trim().to_string();
    if !date_from.is_empty() {
        sql.push_str(" AND t.date >= ?");
    }

    let date_to = filter.date_to.as_deref().unwrap_or("").trim().to_string();
    if !date_to.is_empty() {
        sql.push_str(" AND t.date <= ?");
    }

    sql.push_str(" ORDER BY t.date DESC LIMIT 100");

    let mut query = sqlx::query_as::<_, Transaction>(&sql).bind(user_id.0);
    if let Some(aid) = account_id {
        query = query.bind(aid);
    }
    if !q.is_empty() {
        let pattern = format!("%{q}%");
        query = query.bind(pattern.clone());
        query = query.bind(pattern);
    }
    for lid in &label_ids {
        query = query.bind(lid);
    }
    if !date_from.is_empty() {
        query = query.bind(&date_from);
    }
    if !date_to.is_empty() {
        query = query.bind(&date_to);
    }

    let transactions: Vec<Transaction> = query
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    if transactions.is_empty() {
        return Html(r#"<div class="empty-state">
            <p>No transactions yet. Link a bank account and run a sync to get started.</p>
        </div>"#.to_string());
    }

    // Fetch all allocations for these transactions
    let txn_ids: Vec<i64> = transactions.iter().map(|t| t.id).collect();
    let placeholders = txn_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query_str = format!(
        r#"SELECT a.id, a.transaction_id, a.label_id, a.amount,
                  l.name AS label_name, l.color AS label_color
           FROM allocations a
           JOIN labels l ON a.label_id = l.id
           WHERE a.transaction_id IN ({placeholders})
           ORDER BY a.amount DESC"#
    );
    let mut query = sqlx::query_as::<_, AllocRow>(&query_str);
    for id in &txn_ids {
        query = query.bind(id);
    }
    let allocs: Vec<AllocRow> = query
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let mut rows = String::new();
    for t in &transactions {
        let txn_allocs: Vec<&AllocRow> = allocs
            .iter()
            .filter(|a| a.transaction_id == t.id)
            .collect();
        rows.push_str(&render_row(t, &txn_allocs, base));
    }

    Html(format!(
        r#"<table>
            <thead><tr>
                <th>Date</th>
                <th>Counterparty</th>
                <th>Description</th>
                <th>Labels</th>
                <th>Amount</th>
                <th>Balance</th>
            </tr></thead>
            <tbody>{rows}</tbody>
        </table>"#
    ))
}

// ── Allocation editor (HTMX partial, inserted as row below) ─

async fn alloc_editor(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(txn_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

    // Get transaction amount
    let txn: Option<(f64,)> = sqlx::query_as(
        r#"SELECT t.amount FROM transactions t
           JOIN accounts a ON t.account_id = a.id
           WHERE t.id = ? AND a.user_id = ?"#,
    )
    .bind(txn_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((txn_amount,)) = txn else {
        return Html("".to_string());
    };

    let abs_total = txn_amount.abs();

    // Current allocations
    let allocs: Vec<AllocRow> = sqlx::query_as(
        r#"SELECT a.id, a.transaction_id, a.label_id, a.amount,
                  l.name AS label_name, l.color AS label_color
           FROM allocations a
           JOIN labels l ON a.label_id = l.id
           WHERE a.transaction_id = ?
           ORDER BY a.amount DESC"#,
    )
    .bind(txn_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // All labels for picker
    let labels: Vec<LabelInfo> = sqlx::query_as(
        "SELECT id, name, color FROM labels WHERE user_id = ? ORDER BY name",
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
    let mut options = String::from(r#"<option value="" disabled selected>Choose label...</option>"#);
    for l in &labels {
        // Skip labels already allocated
        let already = allocs.iter().any(|a| a.label_id == l.id);
        if already { continue; }
        options.push_str(&format!(
            r#"<option value="{}">{}</option>"#,
            l.id, l.name,
        ));
    }

    let remaining_class = if remaining.abs() < 0.01 {
        "alloc-remaining-ok"
    } else {
        "alloc-remaining-pending"
    };

    Html(format!(
        r##"<tr id="alloc-editor-{txn_id}" class="alloc-editor-row">
            <td colspan="6">
                <div class="alloc-editor">
                    <div class="alloc-header">
                        <span class="text-sm"><strong>Allocations</strong> — total: <span class="mono">{abs_total:.2}</span></span>
                        <span class="text-sm {remaining_class}">Remaining: <span class="mono">{remaining:.2}</span></span>
                    </div>
                    <div class="alloc-list">{alloc_rows}</div>
                    <form class="alloc-add-form"
                          hx-post="{base}/leanfin/transactions/{txn_id}/allocations/add"
                          hx-target="#alloc-editor-{txn_id}"
                          hx-swap="outerHTML">
                        <select name="label_id" required>{options}</select>
                        <input type="number" name="amount" step="0.01" min="0.01"
                               value="{remaining:.2}" placeholder="Amount" required
                               class="alloc-amount-input mono">
                        <button type="submit" class="btn btn-primary btn-sm">Add</button>
                    </form>
                    <div class="alloc-footer">
                        <button class="btn btn-secondary btn-sm"
                                hx-get="{base}/leanfin/transactions/{txn_id}/row"
                                hx-target="#txn-{txn_id}"
                                hx-swap="outerHTML"
                                onclick="var e=document.getElementById('alloc-editor-{txn_id}');if(e)setTimeout(function(){{e.remove()}},100)">Done</button>
                    </div>
                </div>
            </td>
        </tr>"##
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
    Path(txn_id): Path<i64>,
    Form(form): Form<AddAllocForm>,
) -> Html<String> {
    // Verify label belongs to user
    let owns: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM labels WHERE id = ? AND user_id = ?",
    )
    .bind(form.label_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if owns.is_some() {
        sqlx::query(
            "INSERT INTO allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
        )
        .bind(txn_id)
        .bind(form.label_id)
        .bind(form.amount)
        .execute(&state.pool)
        .await
        .ok();
    }

    // Re-render the editor
    alloc_editor(state, Extension(UserId(user_id.0)), Path(txn_id)).await
}

// ── Delete allocation ────────────────────────────────────────

async fn alloc_delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path((txn_id, alloc_id)): Path<(i64, i64)>,
) -> Html<String> {
    // Delete only if the allocation's label belongs to the user
    sqlx::query(
        r#"DELETE FROM allocations WHERE id = ? AND transaction_id = ?
           AND label_id IN (SELECT id FROM labels WHERE user_id = ?)"#,
    )
    .bind(alloc_id)
    .bind(txn_id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    .ok();

    alloc_editor(state, Extension(UserId(user_id.0)), Path(txn_id)).await
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
               t.created_at
        FROM transactions t
        JOIN accounts a ON t.account_id = a.id
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
           FROM allocations a
           JOIN labels l ON a.label_id = l.id
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

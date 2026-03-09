use axum::{Extension, Json, Router, routing::get};

use super::AppState;
use crate::auth::UserId;
use crate::models::Transaction;

pub fn routes() -> Router<AppState> {
    Router::new().route("/transactions", get(list))
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Json<Vec<Transaction>> {
    let transactions: Vec<Transaction> = sqlx::query_as(
        r#"
        SELECT t.id, t.account_id, t.external_id, t.date, t.amount,
               t.currency, t.description, t.counterparty, t.balance_after,
               t.created_at
        FROM transactions t
        JOIN accounts a ON t.account_id = a.id
        WHERE a.user_id = ?
        ORDER BY t.date DESC
        LIMIT 100
        "#,
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Json(transactions)
}

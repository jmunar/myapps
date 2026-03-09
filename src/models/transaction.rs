use chrono::NaiveDateTime;
use serde::Serialize;

#[derive(sqlx::FromRow, Serialize)]
pub struct Transaction {
    pub id: i64,
    pub account_id: i64,
    pub external_id: String,
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub description: String,
    pub counterparty: Option<String>,
    pub balance_after: Option<f64>,
    pub created_at: NaiveDateTime,
}

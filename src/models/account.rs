use chrono::NaiveDateTime;

#[derive(sqlx::FromRow)]
pub struct Account {
    pub id: i64,
    pub user_id: i64,
    pub bank_name: String,
    pub bank_country: String,
    pub iban: Option<String>,
    pub session_id: String,
    pub account_uid: String,
    pub session_expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

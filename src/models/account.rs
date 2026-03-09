use chrono::NaiveDateTime;

#[derive(sqlx::FromRow)]
pub struct Account {
    pub id: i64,
    pub user_id: i64,
    pub bank_name: String,
    pub iban: Option<String>,
    pub enable_banking_id: String,
    pub access_token_enc: Vec<u8>,
    pub token_expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

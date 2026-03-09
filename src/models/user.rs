use chrono::NaiveDateTime;

#[derive(sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub created_at: NaiveDateTime,
}

#[derive(sqlx::FromRow)]
pub struct Session {
    pub token: String,
    pub user_id: i64,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

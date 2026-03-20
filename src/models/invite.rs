use chrono::NaiveDateTime;

#[derive(sqlx::FromRow)]
pub struct Invite {
    pub token: String,
    pub expires_at: NaiveDateTime,
    pub used_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

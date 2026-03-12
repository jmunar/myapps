#[derive(sqlx::FromRow)]
pub struct Label {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct LabelRule {
    pub id: i64,
    pub label_id: i64,
    pub field: String,
    pub pattern: String,
    pub priority: i64,
}

#[derive(sqlx::FromRow)]
pub struct Allocation {
    pub id: i64,
    pub transaction_id: i64,
    pub label_id: i64,
    pub amount: f64,
}

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
    pub balance_amount: Option<f64>,
    pub balance_currency: Option<String>,
    pub account_type: String,
    pub account_name: Option<String>,
    pub asset_category: Option<String>,
    pub archived: bool,
    pub created_at: NaiveDateTime,
}

impl Account {
    pub fn display_name(&self) -> String {
        if self.is_manual() {
            self.account_name
                .clone()
                .unwrap_or_else(|| self.bank_name.clone())
        } else {
            match &self.iban {
                Some(iban) => format!("{} ({})", self.bank_name, iban),
                None => self.bank_name.clone(),
            }
        }
    }

    pub fn is_manual(&self) -> bool {
        self.account_type == "manual"
    }
}

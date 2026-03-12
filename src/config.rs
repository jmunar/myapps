use std::env;

pub struct Config {
    pub database_url: String,
    pub base_url: Option<String>,
    pub enable_banking_app_id: Option<String>,
    pub enable_banking_key_path: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub bind_addr: String,
    /// URL prefix derived from BASE_URL path (e.g. "/leanfin").
    /// Empty string means served at root.
    pub base_path: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let base_url = env::var("BASE_URL").ok();
        let base_path = base_url
            .as_deref()
            .and_then(|url| url::Url::parse(url).ok())
            .map(|u| u.path().trim_end_matches('/').to_string())
            .unwrap_or_default();

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/leanfin.db".to_string()),
            base_url,
            enable_banking_app_id: env::var("ENABLE_BANKING_APP_ID").ok(),
            enable_banking_key_path: env::var("ENABLE_BANKING_KEY_PATH").ok(),
            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN").ok(),
            telegram_chat_id: env::var("TELEGRAM_CHAT_ID").ok(),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()),
            base_path,
        })
    }

    /// Returns Enable Banking config, or error if not fully configured.
    pub fn require_enable_banking(
        &self,
    ) -> Result<(&str, &str, String), ConfigError> {
        let base_url = self
            .base_url
            .as_deref()
            .ok_or(ConfigError::Missing("BASE_URL"))?;
        let app_id = self
            .enable_banking_app_id
            .as_deref()
            .ok_or(ConfigError::Missing("ENABLE_BANKING_APP_ID"))?;
        let key_path = self
            .enable_banking_key_path
            .as_deref()
            .ok_or(ConfigError::Missing("ENABLE_BANKING_KEY_PATH"))?;
        let redirect_uri = format!("{base_url}/accounts/callback");
        Ok((app_id, key_path, redirect_uri))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    Missing(&'static str),
}

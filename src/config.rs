use std::env;

pub struct Config {
    pub database_url: String,
    pub encryption_key: Option<Vec<u8>>,
    pub enable_banking_app_id: Option<String>,
    pub enable_banking_redirect_uri: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let encryption_key = match env::var("ENCRYPTION_KEY") {
            Ok(hex_str) if !hex_str.is_empty() => {
                let key = hex::decode(&hex_str).map_err(|_| ConfigError::InvalidHex)?;
                if key.len() != 32 {
                    return Err(ConfigError::InvalidKeyLength);
                }
                Some(key)
            }
            _ => None,
        };

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/leanfin.db".to_string()),
            encryption_key,
            enable_banking_app_id: env::var("ENABLE_BANKING_APP_ID").ok(),
            enable_banking_redirect_uri: env::var("ENABLE_BANKING_REDIRECT_URI").ok(),
            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN").ok(),
            telegram_chat_id: env::var("TELEGRAM_CHAT_ID").ok(),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()),
        })
    }

    /// Returns the encryption key, or an error if not configured.
    pub fn require_encryption_key(&self) -> Result<&[u8], ConfigError> {
        self.encryption_key
            .as_deref()
            .ok_or(ConfigError::Missing("ENCRYPTION_KEY"))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    Missing(&'static str),
    #[error("ENCRYPTION_KEY must be valid hex")]
    InvalidHex,
    #[error("ENCRYPTION_KEY must be 32 bytes (64 hex chars)")]
    InvalidKeyLength,
}

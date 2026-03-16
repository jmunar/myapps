use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub base_url: Option<String>,
    pub encryption_key: Option<String>,
    pub vapid_private_key: Option<String>,
    pub vapid_public_key: Option<String>,
    pub vapid_subject: Option<String>,
    pub bind_addr: String,
    /// URL prefix derived from BASE_URL path (e.g. "/myapps").
    /// Empty string means served at root.
    pub base_path: String,
    /// Path to the whisper-cli binary (whisper.cpp).
    pub whisper_cli_path: String,
    /// Directory containing whisper GGML model files.
    pub whisper_models_dir: String,
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
                .unwrap_or_else(|_| "sqlite://data/myapps.db".to_string()),
            base_url,
            encryption_key: env::var("ENCRYPTION_KEY").ok().filter(|s| !s.is_empty()),
            vapid_private_key: env::var("VAPID_PRIVATE_KEY").ok().filter(|s| !s.is_empty()),
            vapid_public_key: env::var("VAPID_PUBLIC_KEY").ok().filter(|s| !s.is_empty()),
            vapid_subject: env::var("VAPID_SUBJECT").ok().filter(|s| !s.is_empty()),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()),
            base_path,
            whisper_cli_path: env::var("WHISPER_CLI_PATH")
                .unwrap_or_else(|_| "whisper-cli".to_string()),
            whisper_models_dir: env::var("WHISPER_MODELS_DIR")
                .unwrap_or_else(|_| "models".to_string()),
        })
    }

    /// Returns the full path to a whisper GGML model file.
    pub fn whisper_model_path(&self, model: &str) -> String {
        format!("{}/ggml-{model}.bin", self.whisper_models_dir)
    }

    /// Scan the models directory for available whisper GGML models.
    /// Returns sorted model names (e.g. ["base-q5_1", "tiny-q5_1"]).
    pub fn available_whisper_models(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.whisper_models_dir) else {
            return Vec::new();
        };
        let mut models: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_prefix("ggml-")
                    .and_then(|s| s.strip_suffix(".bin"))
                    .map(|s| s.to_string())
            })
            .collect();
        models.sort();
        models
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    Missing(&'static str),
}

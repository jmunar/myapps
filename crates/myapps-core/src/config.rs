use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};

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
    /// Optional subset of apps to deploy (app keys). `None` means all apps.
    pub deploy_apps: Option<Vec<String>>,
    /// Base URL of the llama.cpp server (e.g. `http://127.0.0.1:8081`).
    pub llama_server_url: String,
    /// Whether to auto-seed deployed apps when a new user registers via invite.
    pub seed: bool,
    /// Number of days of inactivity before a user is cleaned up. 0 = disabled.
    pub cleanup_inactive_days: i64,
    /// Hash of static assets for cache-busting (computed at startup).
    pub static_version: String,
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
            deploy_apps: env::var("DEPLOY_APPS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.split(',').map(|a| a.trim().to_string()).collect()),
            llama_server_url: env::var("LLAMA_SERVER_URL").unwrap_or_default(),
            seed: env::var("SEED")
                .ok()
                .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
                .unwrap_or(false),
            cleanup_inactive_days: env::var("CLEANUP_INACTIVE_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            static_version: Self::compute_static_version(),
        })
    }

    /// Returns true if the given app key is enabled for this deployment.
    pub fn is_app_deployed(&self, key: &str) -> bool {
        match &self.deploy_apps {
            None => true,
            Some(apps) => apps.iter().any(|a| a == key),
        }
    }

    /// Returns true if the LLM command bar is available.
    pub fn llm_enabled(&self) -> bool {
        !self.llama_server_url.is_empty()
    }

    /// Returns true if whisper transcription is available.
    pub fn whisper_available(&self) -> bool {
        !self.available_whisper_models().is_empty()
    }

    /// Compute a short hash of all files in the `static/` directory.
    fn compute_static_version() -> String {
        Self::compute_static_version_with_extra(&[])
    }

    /// Compute a short hash of `static/` files plus any extra content (e.g. app CSS).
    pub fn compute_static_version_with_extra(extra: &[&str]) -> String {
        let mut hasher = DefaultHasher::new();
        if let Ok(entries) = std::fs::read_dir("static") {
            let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            paths.sort_by_key(|e| e.file_name());
            for entry in paths {
                if let Ok(contents) = std::fs::read(entry.path()) {
                    entry.file_name().hash(&mut hasher);
                    contents.hash(&mut hasher);
                }
            }
        }
        for s in extra {
            s.hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())[..8].to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_deploy_apps(deploy_apps: Option<Vec<String>>) -> Config {
        Config {
            database_url: String::new(),
            base_url: None,
            encryption_key: None,
            vapid_private_key: None,
            vapid_public_key: None,
            vapid_subject: None,
            bind_addr: String::new(),
            base_path: String::new(),
            whisper_cli_path: String::new(),
            whisper_models_dir: String::new(),
            deploy_apps,
            llama_server_url: String::new(),
            seed: false,
            cleanup_inactive_days: 0,
            static_version: String::new(),
        }
    }

    #[test]
    fn is_app_deployed_none_means_all() {
        let config = config_with_deploy_apps(None);
        assert!(config.is_app_deployed("leanfin"));
        assert!(config.is_app_deployed("mindflow"));
        assert!(config.is_app_deployed("anything"));
    }

    #[test]
    fn is_app_deployed_subset_filters() {
        let config = config_with_deploy_apps(Some(vec!["leanfin".into(), "mindflow".into()]));
        assert!(config.is_app_deployed("leanfin"));
        assert!(config.is_app_deployed("mindflow"));
        assert!(!config.is_app_deployed("voice_to_text"));
        assert!(!config.is_app_deployed("classroom_input"));
    }

    #[test]
    fn is_app_deployed_empty_vec_deploys_nothing() {
        let config = config_with_deploy_apps(Some(vec![]));
        assert!(!config.is_app_deployed("leanfin"));
        assert!(!config.is_app_deployed("mindflow"));
    }
}

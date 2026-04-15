use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};

/// An external app shortcut displayed on the launcher (e.g. Vaultwarden, Cockpit).
/// Configured via the `EXTERNAL_APPS` environment variable.
#[derive(Clone, Debug)]
pub struct ExternalApp {
    pub key: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub url: String,
}

/// Parse `EXTERNAL_APPS` env var. Format: `key|name|desc|icon|url;...`
fn parse_external_apps(raw: &str) -> Vec<ExternalApp> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(';')
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.splitn(5, '|').collect();
            if parts.len() == 5 {
                Some(ExternalApp {
                    key: parts[0].trim().to_string(),
                    name: parts[1].trim().to_string(),
                    description: parts[2].trim().to_string(),
                    icon: parts[3].trim().to_string(),
                    url: parts[4].trim().to_string(),
                })
            } else {
                tracing::warn!("Ignoring malformed EXTERNAL_APPS entry: {entry}");
                None
            }
        })
        .collect()
}

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
    /// External app shortcuts shown on the launcher.
    pub external_apps: Vec<ExternalApp>,
    /// Application version (e.g. "0.3.4"), set by the binary crate.
    pub version: String,
    /// Build timestamp (e.g. "2026-04-15 12:00 UTC"), set by the binary crate.
    pub build_timestamp: String,
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
            external_apps: env::var("EXTERNAL_APPS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| parse_external_apps(&s))
                .unwrap_or_default(),
            version: String::new(),
            build_timestamp: String::new(),
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
            external_apps: Vec::new(),
            version: String::new(),
            build_timestamp: String::new(),
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

    #[test]
    fn parse_external_apps_empty() {
        let apps = parse_external_apps("");
        assert!(apps.is_empty());
    }

    #[test]
    fn parse_external_apps_single() {
        let apps =
            parse_external_apps("vault|Vaultwarden|Password manager|🔐|https://vault.example.com");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].key, "vault");
        assert_eq!(apps[0].name, "Vaultwarden");
        assert_eq!(apps[0].description, "Password manager");
        assert_eq!(apps[0].icon, "🔐");
        assert_eq!(apps[0].url, "https://vault.example.com");
    }

    #[test]
    fn parse_external_apps_multiple() {
        let apps = parse_external_apps(
            "vault|Vaultwarden|Passwords|🔐|https://vault.example.com;cockpit|Cockpit|Server|🖥️|https://cockpit.example.com:9090",
        );
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].key, "vault");
        assert_eq!(apps[1].key, "cockpit");
    }

    #[test]
    fn parse_external_apps_skips_malformed() {
        let apps = parse_external_apps("good|App|Desc|🔐|https://example.com;bad|only-two-fields");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].key, "good");
    }

    #[test]
    fn parse_external_apps_trims_whitespace() {
        let apps = parse_external_apps(
            " vault | Vaultwarden | Passwords | 🔐 | https://vault.example.com ",
        );
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].key, "vault");
        assert_eq!(apps[0].name, "Vaultwarden");
        assert_eq!(apps[0].url, "https://vault.example.com");
    }
}

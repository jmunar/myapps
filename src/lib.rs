// Re-export core modules for backwards compatibility with tests
pub use myapps_core::auth;
pub use myapps_core::cli;
pub use myapps_core::command;
pub use myapps_core::config;
pub use myapps_core::db;
pub use myapps_core::i18n;
pub use myapps_core::layout;
pub use myapps_core::models;
pub use myapps_core::registry;
pub use myapps_core::routes;
pub use myapps_core::services;

// Re-export app crates under an `apps` module for test harness compatibility
pub mod apps {
    pub use myapps_form_input as form_input;
    pub use myapps_leanfin as leanfin;
    pub use myapps_mindflow as mindflow;
    pub use myapps_notes as notes;
    pub use myapps_voice_to_text as voice_to_text;
}

use myapps_core::registry::App;

/// All registered app instances.
pub fn all_app_instances() -> Vec<Box<dyn App>> {
    vec![
        Box::new(myapps_leanfin::LeanFinApp),
        Box::new(myapps_mindflow::MindFlowApp),
        Box::new(myapps_voice_to_text::VoiceToTextApp),
        Box::new(myapps_form_input::FormInputApp),
        Box::new(myapps_notes::NotesApp::new()),
    ]
}

/// App instances filtered to those enabled by `DEPLOY_APPS`.
pub fn deployed_app_instances(config: &myapps_core::config::Config) -> Vec<Box<dyn App>> {
    myapps_core::registry::deployed_app_instances(all_app_instances(), config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_app_key_is_prefix_of_another() {
        let apps = all_app_instances();
        let keys: Vec<&str> = apps.iter().map(|a| a.info().key).collect();
        for (i, a) in keys.iter().enumerate() {
            for (j, b) in keys.iter().enumerate() {
                if i != j {
                    let prefix = format!("{a}_");
                    assert!(
                        !b.starts_with(&prefix),
                        "app key {b:?} starts with {a:?}_ — this breaks delete_user_app_data"
                    );
                }
            }
        }
    }
}

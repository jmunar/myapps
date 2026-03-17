use crate::config::Config;

/// Metadata for an application in the launcher.
pub struct AppInfo {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub path: &'static str,
}

/// Returns apps enabled for the current deployment.
/// If `DEPLOY_APPS` is set, only matching apps are returned.
pub fn deployed_apps(config: &Config) -> Vec<AppInfo> {
    all_apps()
        .into_iter()
        .filter(|app| config.is_app_deployed(app.key))
        .collect()
}

/// Returns all available applications.
pub fn all_apps() -> Vec<AppInfo> {
    vec![
        AppInfo {
            key: "leanfin",
            name: "LeanFin",
            description: "Personal expense tracker",
            icon: "$",
            path: "/leanfin",
        },
        AppInfo {
            key: "mindflow",
            name: "MindFlow",
            description: "Thought capture &amp; mind map",
            icon: "\u{1F9E0}",
            path: "/mindflow",
        },
        AppInfo {
            key: "voice_to_text",
            name: "VoiceToText",
            description: "Audio transcription with Whisper",
            icon: "\u{1F3A4}",
            path: "/voice",
        },
        AppInfo {
            key: "classroom_input",
            name: "ClassroomInput",
            description: "Record marks &amp; notes for classrooms",
            icon: "\u{270E}",
            path: "/classroom",
        },
    ]
}

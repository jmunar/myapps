use myapps_core::registry::App;

/// All registered app instances.
pub fn all_app_instances() -> Vec<Box<dyn App>> {
    vec![
        Box::new(myapps_leanfin::LeanFinApp),
        Box::new(myapps_mindflow::MindFlowApp),
        Box::new(myapps_voice_to_text::VoiceToTextApp),
        Box::new(myapps_form_input::FormInputApp),
        Box::new(myapps_notes::NotesApp::new()),
        Box::new(myapps_file_clipboard::FileClipboardApp),
    ]
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

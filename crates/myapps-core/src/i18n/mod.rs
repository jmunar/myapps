pub mod en;
pub mod es;

/// Supported languages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        match code {
            "es" => Self::Es,
            _ => Self::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// Returns the translations for the given language.
pub fn t(lang: Lang) -> &'static Translations {
    match lang {
        Lang::En => &en::EN,
        Lang::Es => &es::ES,
    }
}

/// Shared translations used by core infrastructure (auth, layout, command bar).
/// App-specific translations live in each app's `i18n` module.
/// Adding a field here forces both `en.rs` and `es.rs` to be updated (compiler error).
pub struct Translations {
    // ── Shared / Layout ──────────────────────────────────────
    pub log_out: &'static str,

    // ── Login ────────────────────────────────────────────────
    pub login_title: &'static str,
    pub login_subtitle: &'static str,
    pub login_username: &'static str,
    pub login_password: &'static str,
    pub login_submit: &'static str,
    pub login_invalid: &'static str,
    pub login_error: &'static str,

    // ── Invite / Register ─────────────────────────────────────
    pub invite_title: &'static str,
    pub invite_subtitle: &'static str,
    pub invite_username: &'static str,
    pub invite_password: &'static str,
    pub invite_confirm_password: &'static str,
    pub invite_submit: &'static str,
    pub invite_invalid: &'static str,
    pub invite_expired: &'static str,
    pub invite_used: &'static str,
    pub invite_passwords_mismatch: &'static str,
    pub invite_username_taken: &'static str,

    // ── Launcher ─────────────────────────────────────────────
    pub launcher_title: &'static str,
    pub launcher_subtitle: &'static str,
    pub launcher_empty_prefix: &'static str,
    pub launcher_empty_suffix: &'static str,
    pub launcher_configure: &'static str,
    pub launcher_toggle_visibility: &'static str,
    pub launcher_done: &'static str,
    pub launcher_hide: &'static str,
    pub launcher_show: &'static str,
    pub launcher_notif_enabled: &'static str,
    pub launcher_notif_blocked: &'static str,
    pub launcher_notif_blocked_settings: &'static str,
    pub launcher_notif_enable: &'static str,
    pub launcher_external_badge: &'static str,

    // ── Language selector ────────────────────────────────────
    pub language_label: &'static str,

    // ── Command Bar ─────────────────────────────────────────
    pub cmd_placeholder: &'static str,
    pub cmd_busy: &'static str,
    pub cmd_error: &'static str,
    pub cmd_not_configured: &'static str,
    pub cmd_no_actions: &'static str,
    pub cmd_no_params: &'static str,
    pub cmd_confidence: &'static str,
    pub cmd_confirm: &'static str,
    pub cmd_cancel: &'static str,

    // ── Command Bar (voice) ──────────────────────────────────
    pub cmd_record: &'static str,
    pub cmd_voice_command: &'static str,
    pub cmd_recording: &'static str,
    pub cmd_transcribing: &'static str,
    pub cmd_interpreting: &'static str,
    pub cmd_edit: &'static str,
    pub cmd_edit_done: &'static str,
    pub cmd_transcribe_error: &'static str,
    pub cmd_mic_no_permission: &'static str,
}

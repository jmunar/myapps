use super::Translations;

pub const EN: Translations = Translations {
    // ── Shared / Layout ──────────────────────────────────────
    log_out: "Log out",

    // ── Login ────────────────────────────────────────────────
    login_title: "MyApps — Login",
    login_subtitle: "Your personal platform",
    login_username: "Username",
    login_password: "Password",
    login_submit: "Log in",
    login_invalid: "Invalid credentials",
    login_error: "Internal error",

    // ── Invite / Register ─────────────────────────────────────
    invite_title: "MyApps — Register",
    invite_subtitle: "Create your account",
    invite_username: "Username",
    invite_password: "Password",
    invite_confirm_password: "Confirm password",
    invite_submit: "Create account",
    invite_invalid: "This invite link is invalid.",
    invite_expired: "This invite link has expired.",
    invite_used: "This invite link has already been used.",
    invite_passwords_mismatch: "Passwords do not match.",
    invite_username_taken: "This username is already taken.",

    // ── Launcher ─────────────────────────────────────────────
    launcher_title: "My Apps",
    launcher_subtitle: "Choose an application",
    launcher_empty_prefix: "No apps visible. Click ",
    launcher_empty_suffix: " to configure.",
    launcher_configure: "Configure apps",
    launcher_toggle_visibility: "Toggle app visibility",
    launcher_done: "Done",
    launcher_hide: "Hide app",
    launcher_show: "Show app",
    launcher_notif_enabled: "Notifications enabled",
    launcher_notif_blocked: "Notifications blocked",
    launcher_notif_blocked_settings: "Notifications blocked (check browser settings)",
    launcher_notif_enable: "Enable notifications",

    // ── Language selector ────────────────────────────────────
    language_label: "Language",

    // ── Command Bar ─────────────────────────────────────────
    cmd_placeholder: "Type a command\u{2026}",
    cmd_busy: "Model busy, try again shortly.",
    cmd_error: "Command error",
    cmd_not_configured: "Command bar not configured.",
    cmd_no_actions: "No actions available.",
    cmd_no_params: "No parameters",
    cmd_confidence: "Confidence",
    cmd_confirm: "Confirm",
    cmd_cancel: "Cancel",

    // ── Command Bar (voice) ──────────────────────────────────
    cmd_record: "Record command",
    cmd_voice_command: "Voice Command",
    cmd_recording: "Recording\u{2026}",
    cmd_transcribing: "Transcribing\u{2026}",
    cmd_interpreting: "Interpreting\u{2026}",
    cmd_edit: "Edit",
    cmd_edit_done: "Done",
    cmd_transcribe_error: "Transcription failed",
    cmd_mic_no_permission: "Microphone access denied",
};

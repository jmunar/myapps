use super::Translations;

pub const ES: Translations = Translations {
    // ── Shared / Layout ──────────────────────────────────────
    log_out: "Cerrar sesión",

    // ── Login ────────────────────────────────────────────────
    login_title: "MyApps — Iniciar sesión",
    login_subtitle: "Tu plataforma personal",
    login_username: "Usuario",
    login_password: "Contraseña",
    login_submit: "Iniciar sesión",
    login_invalid: "Credenciales inválidas",
    login_error: "Error interno",

    // ── Invite / Register ─────────────────────────────────────
    invite_title: "MyApps — Registro",
    invite_subtitle: "Crea tu cuenta",
    invite_username: "Usuario",
    invite_password: "Contraseña",
    invite_confirm_password: "Confirmar contraseña",
    invite_submit: "Crear cuenta",
    invite_invalid: "Este enlace de invitación no es válido.",
    invite_expired: "Este enlace de invitación ha expirado.",
    invite_used: "Este enlace de invitación ya ha sido utilizado.",
    invite_passwords_mismatch: "Las contraseñas no coinciden.",
    invite_username_taken: "Este nombre de usuario ya está en uso.",

    // ── Launcher ─────────────────────────────────────────────
    launcher_title: "Mis Apps",
    launcher_subtitle: "Elige una aplicación",
    launcher_empty_prefix: "No hay apps visibles. Pulsa ",
    launcher_empty_suffix: " para configurar.",
    launcher_configure: "Configurar apps",
    launcher_toggle_visibility: "Mostrar/ocultar apps",
    launcher_done: "Hecho",
    launcher_hide: "Ocultar app",
    launcher_show: "Mostrar app",
    launcher_notif_enabled: "Notificaciones activadas",
    launcher_notif_blocked: "Notificaciones bloqueadas",
    launcher_notif_blocked_settings: "Notificaciones bloqueadas (revisa la configuración del navegador)",
    launcher_notif_enable: "Activar notificaciones",
    launcher_external_badge: "Abre en nueva pesta\u{f1}a",

    // ── Language selector ────────────────────────────────────
    language_label: "Idioma",

    // ── Command Bar ─────────────────────────────────────────
    cmd_placeholder: "Escribe un comando\u{2026}",
    cmd_busy: "Modelo ocupado, int\u{e9}ntalo en un momento.",
    cmd_error: "Error de comando",
    cmd_not_configured: "Barra de comandos no configurada.",
    cmd_no_actions: "No hay acciones disponibles.",
    cmd_no_params: "Sin par\u{e1}metros",
    cmd_confidence: "Confianza",
    cmd_confirm: "Confirmar",
    cmd_cancel: "Cancelar",

    // ── Command Bar (voice) ──────────────────────────────────
    cmd_record: "Grabar comando",
    cmd_voice_command: "Comando de voz",
    cmd_recording: "Grabando\u{2026}",
    cmd_transcribing: "Transcribiendo\u{2026}",
    cmd_interpreting: "Interpretando\u{2026}",
    cmd_edit: "Editar",
    cmd_edit_done: "Listo",
    cmd_transcribe_error: "Error de transcripci\u{f3}n",
    cmd_mic_no_permission: "Acceso al micr\u{f3}fono denegado",
};

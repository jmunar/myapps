use crate::config::Config;
use crate::i18n::{self, Lang};

/// Swipe between neighbouring nav tabs on a phone. Inlined into every page so
/// it needs no round trip, and kept in its own file so the braces need no
/// escaping.
///
/// It goes in `<head>`, not at the end of `<body>`: it arms the slide-in of the
/// page just swiped to, which has to be set before `<main>` first paints. The
/// script touches nothing but `<html>` until a finger lands on the screen.
const NAV_SWIPE_JS: &str = include_str!("../../../static/nav-swipe.js");

/// Web-push subscription, shared with the launcher's enable-notifications
/// button. Inlined on every page so `window.MyAppsPush` is always defined by
/// the time a page script runs.
const PUSH_JS: &str = include_str!("../../../static/push.js");

/// Service-worker registration. Reads the base path from `<html data-base>`,
/// so nothing is interpolated into it.
const SW_REGISTER_JS: &str = include_str!("../../../static/sw-register.js");

/// Voice command bar. Reads its base path and status strings from the DOM, so
/// the Rust side interpolates nothing into it.
const COMMAND_BAR_JS: &str = include_str!("../../../static/command-bar.js");

/// A single nav item for the shared layout.
pub struct NavItem {
    pub href: String,
    pub label: String,
    pub active: bool,
    /// If true, this item is rendered right-aligned (e.g. "Log out").
    pub right: bool,
}

/// Build the nav for a page inside an app.
///
/// Every app's nav has the same three parts: a brand link back to the app
/// root, one item per tab, and Log out pinned right. `tabs` supplies the
/// middle: `(href suffix, label, active key)`, where an empty suffix is the
/// app root and `active` matches the key of the tab being shown.
pub fn app_nav(
    base: &str,
    root: &str,
    brand: &str,
    active: &str,
    lang: Lang,
    tabs: &[(&str, &str, &str)],
) -> Vec<NavItem> {
    let mut items = vec![NavItem {
        href: format!("{base}{root}"),
        label: brand.to_string(),
        active: false,
        right: false,
    }];
    items.extend(tabs.iter().map(|(suffix, label, key)| NavItem {
        href: format!("{base}{root}{suffix}"),
        label: (*label).to_string(),
        active: active == *key,
        right: false,
    }));
    items.push(NavItem {
        href: format!("{base}/logout"),
        label: i18n::t(lang).log_out.to_string(),
        active: false,
        right: true,
    });
    items
}

/// Render a full HTML page shell with nav and body content.
/// The command bar is automatically included when the LLM is configured.
pub fn render_page(
    title: &str,
    nav_items: &[NavItem],
    body_html: &str,
    config: &Config,
    lang: Lang,
) -> String {
    let base_path = &config.base_path;
    let sv = &config.static_version;
    let lang_code = lang.code();

    let mut nav_html = String::new();
    for item in nav_items {
        let active = if item.active { " class=\"active\"" } else { "" };
        if item.right {
            nav_html.push_str(&format!(
                r#"<a href="{href}" class="nav-right">{label}</a>"#,
                href = item.href,
                label = item.label,
            ));
        } else {
            nav_html.push_str(&format!(
                r#"<a href="{href}"{active}>{label}</a>"#,
                href = item.href,
                label = item.label,
            ));
        }
    }

    let command_bar = if config.llm_enabled() && config.whisper_available() {
        let t = i18n::t(lang);
        format!(
            r##"<button id="cmd-mic" class="cmd-mic-btn" title="{cmd_record}">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/></svg>
</button>
<div id="cmd-swipe-hint" class="cmd-swipe-hint">&larr;</div>
<div id="cmd-window" class="cmd-window" style="display:none"
     data-transcribing="{cmd_transcribing}"
     data-interpreting="{cmd_interpreting}"
     data-mic-error="{cmd_mic_no_permission}">
    <div class="cmd-window-header">
        <span>{cmd_voice_command}</span>
        <button id="cmd-close" class="cmd-window-close">&times;</button>
    </div>
    <div id="cmd-status" class="cmd-status"></div>
    <div id="cmd-transcription" class="cmd-transcription" style="display:none">
        <span id="cmd-text"></span>
        <button id="cmd-edit-btn" class="cmd-edit-btn" title="{cmd_edit}">&#9998;</button>
    </div>
    <div id="cmd-edit-area" class="cmd-edit-area" style="display:none">
        <textarea id="cmd-edit-input" rows="2"></textarea>
        <button id="cmd-edit-done" class="btn btn-primary">{cmd_edit_done}</button>
    </div>
    <div id="command-result"></div>
</div>
<script>{COMMAND_BAR_JS}</script>"##,
            cmd_record = t.cmd_record,
            cmd_voice_command = t.cmd_voice_command,
            cmd_transcribing = t.cmd_transcribing,
            cmd_interpreting = t.cmd_interpreting,
            cmd_edit = t.cmd_edit,
            cmd_edit_done = t.cmd_edit_done,
            cmd_mic_no_permission = t.cmd_mic_no_permission,
        )
    } else {
        String::new()
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang_code}" data-base="{base_path}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base_path}/static/core.css?v={sv}">
    <link rel="stylesheet" href="{base_path}/static/apps.css?v={sv}">
    <link rel="manifest" href="{base_path}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base_path}/static/icon.svg?v={sv}">
    <link rel="apple-touch-icon" href="{base_path}/static/icon.svg?v={sv}">
    <script src="{base_path}/static/htmx.min.js?v={sv}"></script>
    <script>{NAV_SWIPE_JS}</script>
</head>
<body>
    <script>{PUSH_JS}</script>
    <script>{SW_REGISTER_JS}</script>
    <nav>
        <a href="{base_path}/" class="brand">MyApps</a>
        {nav_html}
        </nav>
    <main>
        {body_html}
    </main>
    {command_bar}
</body>
</html>"##
    )
}

use axum::{
    Extension, Form, Router,
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;
use std::collections::HashMap;

use super::AppState;
use crate::apps::registry::{AppInfo, deployed_apps};
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::{NavItem, render_page};
use crate::models::user_app_visibility;

const TARGET: &str = "#launcher-area";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/launcher/edit", get(edit_mode))
        .route("/launcher/grid", get(grid_fragment))
        .route("/launcher/visibility", post(set_visibility))
}

fn push_script(base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    let notif_enabled = t.launcher_notif_enabled;
    let notif_blocked = t.launcher_notif_blocked;
    let notif_blocked_settings = t.launcher_notif_blocked_settings;
    let notif_enable = t.launcher_notif_enable;
    format!(
        r#"<div id="push-status" style="text-align:center;margin-top:1.5rem;font-size:0.9rem;color:#888;"></div>
        <script>
        (function() {{
            var el = document.getElementById("push-status");
            if (!("Notification" in window) || !("PushManager" in window)) return;
            if (Notification.permission === "granted") {{
                el.textContent = "{notif_enabled}";
            }} else if (Notification.permission === "denied") {{
                el.textContent = "{notif_blocked_settings}";
            }} else {{
                var btn = document.createElement("button");
                btn.textContent = "{notif_enable}";
                btn.className = "btn btn-secondary";
                btn.onclick = function() {{
                    Notification.requestPermission().then(function(perm) {{
                        if (perm === "granted") {{
                            el.textContent = "{notif_enabled}";
                            navigator.serviceWorker.ready.then(function(reg) {{
                                fetch("{base}/push/vapid-key").then(function(r) {{ return r.text(); }}).then(function(key) {{
                                    var padding = (4 - key.length % 4) % 4;
                                    var b64 = key.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat(padding);
                                    var raw = atob(b64);
                                    var arr = new Uint8Array(raw.length);
                                    for (var i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
                                    return reg.pushManager.subscribe({{ userVisibleOnly: true, applicationServerKey: arr }});
                                }}).then(function(sub) {{
                                    if (!sub) return;
                                    var key = sub.getKey("p256dh");
                                    var auth = sub.getKey("auth");
                                    var body = {{
                                        endpoint: sub.endpoint,
                                        p256dh: btoa(String.fromCharCode.apply(null, new Uint8Array(key))).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/,""),
                                        auth: btoa(String.fromCharCode.apply(null, new Uint8Array(auth))).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/,"")
                                    }};
                                    fetch("{base}/push/subscribe", {{
                                        method: "POST",
                                        headers: {{ "Content-Type": "application/json" }},
                                        body: JSON.stringify(body)
                                    }});
                                }});
                            }});
                        }} else {{
                            el.textContent = "{notif_blocked}";
                        }}
                    }});
                }};
                el.appendChild(btn);
            }}
        }})();
        </script>"#
    )
}

fn render_grid_normal(
    apps: &[AppInfo],
    visibility: &HashMap<String, bool>,
    base: &str,
    lang: Lang,
) -> String {
    let t = i18n::t(lang);
    let visible_apps: Vec<&AppInfo> = apps
        .iter()
        .filter(|app| *visibility.get(app.key).unwrap_or(&true))
        .collect();

    if visible_apps.is_empty() {
        return format!(
            r#"<div class="empty-state">
                <p>{prefix}<button class="launcher-edit-btn"
                    hx-get="{base}/launcher/edit" hx-target="{target}" hx-swap="innerHTML">&#9881;</button>{suffix}</p>
            </div>"#,
            prefix = t.launcher_empty_prefix,
            suffix = t.launcher_empty_suffix,
            target = TARGET,
        );
    }

    let cards: String = visible_apps
        .iter()
        .map(|app| {
            format!(
                r#"<a href="{base}{path}" class="launcher-card">
                    <div class="launcher-icon">{icon}</div>
                    <div class="launcher-info">
                        <h2>{name}</h2>
                        <p>{desc}</p>
                    </div>
                </a>"#,
                path = app.path,
                icon = app.icon,
                name = app.name,
                desc = t.app_description(app.key),
            )
        })
        .collect();

    format!(r#"<div class="launcher-grid">{cards}</div>"#)
}

fn render_grid_edit(
    apps: &[AppInfo],
    visibility: &HashMap<String, bool>,
    base: &str,
    lang: Lang,
) -> String {
    let t = i18n::t(lang);
    let cards: String = apps
        .iter()
        .map(|app| {
            let visible = *visibility.get(app.key).unwrap_or(&true);
            let hidden_class = if visible { "" } else { " hidden" };
            let toggle_val = if visible { "0" } else { "1" };
            let eye = if visible {
                "&#128065;"
            } else {
                "&#128065;&#8205;&#128488;"
            };
            let title = if visible {
                t.launcher_hide
            } else {
                t.launcher_show
            };
            format!(
                r#"<div class="launcher-card launcher-card-edit{hidden_class}" id="card-{key}">
                    <div class="launcher-icon">{icon}</div>
                    <div class="launcher-info">
                        <h2>{name}</h2>
                        <p>{desc}</p>
                    </div>
                    <button class="launcher-toggle"
                        hx-post="{base}/launcher/visibility"
                        hx-vals='{{"app_key":"{key}","visible":"{toggle_val}"}}'
                        hx-target="{target}"
                        hx-swap="innerHTML"
                        title="{title}">{eye}</button>
                </div>"#,
                key = app.key,
                icon = app.icon,
                name = app.name,
                desc = t.app_description(app.key),
                target = TARGET,
            )
        })
        .collect();

    format!(r#"<div class="launcher-grid">{cards}</div>"#)
}

fn render_header_normal(base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    format!(
        r#"<div class="page-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div>
                <h1>{title}</h1>
                <p>{subtitle}</p>
            </div>
            <button class="launcher-edit-btn" hx-get="{base}/launcher/edit" hx-target="{target}" hx-swap="innerHTML" title="{configure}">&#9881;</button>
        </div>"#,
        title = t.launcher_title,
        subtitle = t.launcher_subtitle,
        configure = t.launcher_configure,
        target = TARGET,
    )
}

fn render_header_edit(base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    format!(
        r#"<div class="page-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div>
                <h1>{title}</h1>
                <p>{toggle}</p>
            </div>
            <button class="launcher-done-btn btn btn-primary btn-sm" hx-get="{base}/launcher/grid" hx-target="{target}" hx-swap="innerHTML">{done}</button>
        </div>"#,
        title = t.launcher_title,
        toggle = t.launcher_toggle_visibility,
        done = t.launcher_done,
        target = TARGET,
    )
}

fn render_lang_selector(base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    let en_selected = if lang == Lang::En { " selected" } else { "" };
    let es_selected = if lang == Lang::Es { " selected" } else { "" };
    format!(
        r#"<form method="POST" action="{base}/settings/language" style="text-align:center;margin-top:1rem">
            <input type="hidden" name="redirect" value="{base}/">
            <label style="font-size:0.875rem;color:var(--text-secondary)">{label}:
                <select name="language" onchange="this.form.submit()" style="margin-left:0.25rem">
                    <option value="en"{en_selected}>English</option>
                    <option value="es"{es_selected}>Español</option>
                </select>
            </label>
        </form>"#,
        label = t.language_label,
    )
}

async fn index(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let nav = vec![NavItem {
        href: format!("{base}/logout"),
        label: t.log_out.to_string(),
        active: false,
        right: true,
    }];

    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = deployed_apps(&state.config);

    let header = render_header_normal(base, lang);
    let grid = render_grid_normal(&apps, &visibility, base, lang);
    let push = push_script(base, lang);
    let lang_sel = render_lang_selector(base, lang);

    let body = format!(r#"<div id="launcher-area">{header}{grid}</div>{lang_sel}{push}"#);
    Html(render_page("MyApps", &nav, &body, &state.config, lang))
}

async fn edit_mode(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = deployed_apps(&state.config);

    let header = render_header_edit(base, lang);
    let grid = render_grid_edit(&apps, &visibility, base, lang);

    Html(format!("{header}{grid}"))
}

async fn grid_fragment(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = deployed_apps(&state.config);

    let header = render_header_normal(base, lang);
    let grid = render_grid_normal(&apps, &visibility, base, lang);

    Html(format!("{header}{grid}"))
}

#[derive(Deserialize)]
struct VisibilityForm {
    app_key: String,
    visible: String,
}

async fn set_visibility(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Form(form): Form<VisibilityForm>,
) -> Html<String> {
    let visible = form.visible != "0";

    // Validate app_key against registry
    let apps = deployed_apps(&state.config);
    if apps.iter().any(|a| a.key == form.app_key) {
        let _ =
            user_app_visibility::set_visibility(&state.pool, user_id, &form.app_key, visible).await;
    }

    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;

    // Return edit mode view so user can keep toggling
    let header = render_header_edit(base, lang);
    let grid = render_grid_edit(&apps, &visibility, base, lang);

    Html(format!("{header}{grid}"))
}

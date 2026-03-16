use axum::{
    Extension, Form, Router,
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;
use std::collections::HashMap;

use super::AppState;
use crate::apps::registry::{AppInfo, all_apps};
use crate::auth::UserId;
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

fn push_script(base: &str) -> String {
    format!(
        r#"<div id="push-status" style="text-align:center;margin-top:1.5rem;font-size:0.9rem;color:#888;"></div>
        <script>
        (function() {{
            var el = document.getElementById("push-status");
            if (!("Notification" in window) || !("PushManager" in window)) return;
            if (Notification.permission === "granted") {{
                el.textContent = "Notifications enabled";
            }} else if (Notification.permission === "denied") {{
                el.textContent = "Notifications blocked (check browser settings)";
            }} else {{
                var btn = document.createElement("button");
                btn.textContent = "Enable notifications";
                btn.className = "btn btn-secondary";
                btn.onclick = function() {{
                    Notification.requestPermission().then(function(perm) {{
                        if (perm === "granted") {{
                            el.textContent = "Notifications enabled";
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
                            el.textContent = "Notifications blocked";
                        }}
                    }});
                }};
                el.appendChild(btn);
            }}
        }})();
        </script>"#
    )
}

fn render_grid_normal(apps: &[AppInfo], visibility: &HashMap<String, bool>, base: &str) -> String {
    let visible_apps: Vec<&AppInfo> = apps
        .iter()
        .filter(|app| *visibility.get(app.key).unwrap_or(&true))
        .collect();

    if visible_apps.is_empty() {
        return format!(
            r#"<div class="empty-state">
                <p>No apps visible. Click <button class="launcher-edit-btn"
                    hx-get="{base}/launcher/edit" hx-target="{target}" hx-swap="innerHTML">&#9881;</button> to configure.</p>
            </div>"#,
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
                desc = app.description,
            )
        })
        .collect();

    format!(r#"<div class="launcher-grid">{cards}</div>"#)
}

fn render_grid_edit(apps: &[AppInfo], visibility: &HashMap<String, bool>, base: &str) -> String {
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
            let title = if visible { "Hide app" } else { "Show app" };
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
                desc = app.description,
                target = TARGET,
            )
        })
        .collect();

    format!(r#"<div class="launcher-grid">{cards}</div>"#)
}

fn render_header_normal(base: &str) -> String {
    format!(
        r#"<div class="page-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div>
                <h1>My Apps</h1>
                <p>Choose an application</p>
            </div>
            <button class="launcher-edit-btn" hx-get="{base}/launcher/edit" hx-target="{target}" hx-swap="innerHTML" title="Configure apps">&#9881;</button>
        </div>"#,
        target = TARGET,
    )
}

fn render_header_edit(base: &str) -> String {
    format!(
        r#"<div class="page-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div>
                <h1>My Apps</h1>
                <p>Toggle app visibility</p>
            </div>
            <button class="launcher-done-btn btn btn-primary btn-sm" hx-get="{base}/launcher/grid" hx-target="{target}" hx-swap="innerHTML">Done</button>
        </div>"#,
        target = TARGET,
    )
}

async fn index(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;
    let nav = vec![NavItem {
        href: format!("{base}/logout"),
        label: "Log out",
        active: false,
    }];

    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = all_apps();

    let header = render_header_normal(base);
    let grid = render_grid_normal(&apps, &visibility, base);
    let push = push_script(base);

    let body = format!(r#"<div id="launcher-area">{header}{grid}</div>{push}"#);
    Html(render_page("MyApps", &nav, &body, base))
}

async fn edit_mode(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = all_apps();

    let header = render_header_edit(base);
    let grid = render_grid_edit(&apps, &visibility, base);

    Html(format!("{header}{grid}"))
}

async fn grid_fragment(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let apps = all_apps();

    let header = render_header_normal(base);
    let grid = render_grid_normal(&apps, &visibility, base);

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
    Form(form): Form<VisibilityForm>,
) -> Html<String> {
    let visible = form.visible != "0";

    // Validate app_key against registry
    let apps = all_apps();
    if apps.iter().any(|a| a.key == form.app_key) {
        let _ =
            user_app_visibility::set_visibility(&state.pool, user_id, &form.app_key, visible).await;
    }

    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;

    // Return edit mode view so user can keep toggling
    let header = render_header_edit(base);
    let grid = render_grid_edit(&apps, &visibility, base);

    Html(format!("{header}{grid}"))
}

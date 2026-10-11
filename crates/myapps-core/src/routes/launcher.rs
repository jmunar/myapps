use axum::{
    Extension, Form, Router,
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;
use std::collections::HashMap;

use super::AppState;
use crate::auth::UserId;
use crate::components::html_escape;
use crate::config::{Config, ExternalApp};
use crate::i18n::{self, Lang};
use crate::layout::{NavItem, render_page};
use crate::models::{user_app_order, user_app_visibility};
use crate::registry::App;

const TARGET: &str = "#launcher-area";

/// Notification opt-in behaviour for the launcher. Relies on `window.MyAppsPush`
/// from `static/push.js`, which layout.rs inlines on every page.
const LAUNCHER_PUSH_JS: &str = include_str!("../../../../static/launcher-push.js");

/// Drag-to-reorder for edit mode. Delegated from the document, since the edit
/// grid arrives later by htmx swap.
const LAUNCHER_ORDER_JS: &str = include_str!("../../../../static/launcher-order.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/launcher/edit", get(edit_mode))
        .route("/launcher/grid", get(grid_fragment))
        .route("/launcher/visibility", post(set_visibility))
        .route("/launcher/order", post(set_order))
}

/// Notification status line plus the enable button. Behaviour lives in
/// `static/launcher-push.js`; the labels reach it as data attributes, so the
/// script is the same bytes for every language.
fn push_script(_base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    format!(
        r#"<div id="push-status"
             style="text-align:center;margin-top:1.5rem;font-size:0.9rem;color:#888;"
             data-enabled="{enabled}"
             data-blocked="{blocked}"
             data-blocked-settings="{blocked_settings}"
             data-enable="{enable}"></div>
        <script>{LAUNCHER_PUSH_JS}</script>"#,
        enabled = html_escape(t.launcher_notif_enabled),
        blocked = html_escape(t.launcher_notif_blocked),
        blocked_settings = html_escape(t.launcher_notif_blocked_settings),
        enable = html_escape(t.launcher_notif_enable),
    )
}

/// A launcher card: one of ours, or an `EXTERNAL_APPS` shortcut.
enum Entry<'a> {
    Internal(&'a dyn App),
    External(&'a ExternalApp),
}

impl Entry<'_> {
    fn key(&self) -> &str {
        match self {
            Entry::Internal(app) => app.info().key,
            Entry::External(ext) => &ext.key,
        }
    }

    /// The icon and the name/description block every card variant shares.
    fn body(&self, lang: Lang) -> String {
        let (icon, name, desc) = match self {
            Entry::Internal(app) => {
                let info = app.info();
                (info.icon, info.name.to_string(), app.description(lang))
            }
            Entry::External(ext) => (
                ext.icon.as_str(),
                format!(
                    r#"{} <span class="external-badge">&#8599;</span>"#,
                    ext.name
                ),
                ext.description.as_str(),
            ),
        };
        format!(
            r#"<div class="launcher-icon">{icon}</div>
                    <div class="launcher-info">
                        <h2>{name}</h2>
                        <p>{desc}</p>
                    </div>"#
        )
    }
}

/// Every card in the user's saved order. Cards missing from it (apps added
/// since, or no saved order at all) follow in registry order, internal apps
/// before external ones.
fn ordered_entries<'a>(
    apps: &'a [Box<dyn App>],
    external_apps: &'a [ExternalApp],
    order: &[String],
) -> Vec<Entry<'a>> {
    let mut entries: Vec<Entry> = apps
        .iter()
        .map(|app| Entry::Internal(app.as_ref()))
        .chain(external_apps.iter().map(Entry::External))
        .collect();
    // Stable, so the unsaved ones keep their relative order at the end.
    entries.sort_by_key(|e| {
        order
            .iter()
            .position(|k| k == e.key())
            .unwrap_or(usize::MAX)
    });
    entries
}

fn render_grid_normal(
    entries: &[Entry],
    visibility: &HashMap<String, bool>,
    base: &str,
    lang: Lang,
) -> String {
    let t = i18n::t(lang);
    let cards: String = entries
        .iter()
        .filter(|e| *visibility.get(e.key()).unwrap_or(&true))
        .map(|entry| {
            let body = entry.body(lang);
            match entry {
                Entry::Internal(app) => format!(
                    r#"<a href="{base}{path}" class="launcher-card">
                    {body}
                </a>"#,
                    path = app.info().path,
                ),
                Entry::External(ext) => format!(
                    r#"<a href="{url}" target="_blank" rel="noopener noreferrer" class="launcher-card launcher-card-external" title="{badge}">
                    {body}
                </a>"#,
                    url = ext.url,
                    badge = t.launcher_external_badge,
                ),
            }
        })
        .collect();

    if cards.is_empty() {
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

    format!(r#"<div class="launcher-grid">{cards}</div>"#)
}

/// Every card, hidden ones dimmed, each with a drag handle and an eye toggle.
/// The dragging is `static/launcher-order.js`, which finds the cards and
/// handles by their `data-launcher-*` attributes.
fn render_grid_edit(
    entries: &[Entry],
    visibility: &HashMap<String, bool>,
    base: &str,
    lang: Lang,
) -> String {
    let t = i18n::t(lang);
    let cards: String = entries
        .iter()
        .map(|entry| {
            let key = entry.key();
            let visible = *visibility.get(key).unwrap_or(&true);
            let external_class = match entry {
                Entry::Internal(_) => "",
                Entry::External(_) => " launcher-card-external",
            };
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
                r#"<div class="launcher-card launcher-card-edit{external_class}{hidden_class}" id="card-{key}" data-launcher-key="{key}">
                    <button type="button" class="launcher-handle" data-launcher-handle
                        title="{reorder}" aria-label="{reorder}">&#10303;</button>
                    {body}
                    <button class="launcher-toggle"
                        hx-post="{base}/launcher/visibility"
                        hx-vals='{{"app_key":"{key}","visible":"{toggle_val}"}}'
                        hx-target="{target}"
                        hx-swap="innerHTML"
                        title="{title}">{eye}</button>
                </div>"#,
                body = entry.body(lang),
                reorder = t.launcher_reorder,
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
        toggle = t.launcher_edit_hint,
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

fn render_version_footer(config: &Config) -> String {
    if config.version.is_empty() {
        return String::new();
    }
    let ts = if config.build_timestamp.is_empty() {
        String::new()
    } else {
        format!(" &middot; {}", config.build_timestamp)
    };
    format!(
        r#"<p class="version-footer">v{version}{ts}</p>"#,
        version = config.version,
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
    let order = user_app_order::get_order(&state.pool, user_id).await;
    let entries = ordered_entries(&state.apps, &state.config.external_apps, &order);

    let header = render_header_normal(base, lang);
    let grid = render_grid_normal(&entries, &visibility, base, lang);
    let push = push_script(base, lang);
    let lang_sel = render_lang_selector(base, lang);
    let version_footer = render_version_footer(&state.config);

    let body = format!(
        r#"<div id="launcher-area">{header}{grid}</div>{lang_sel}{push}{version_footer}
        <script>{LAUNCHER_ORDER_JS}</script>"#
    );
    Html(render_page("MyApps", &nav, &body, &state.config, lang))
}

async fn edit_mode(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    Html(edit_fragment(&state, user_id, lang).await)
}

async fn grid_fragment(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let order = user_app_order::get_order(&state.pool, user_id).await;
    let entries = ordered_entries(&state.apps, &state.config.external_apps, &order);

    let header = render_header_normal(base, lang);
    let grid = render_grid_normal(&entries, &visibility, base, lang);

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

    if is_known_key(&state, &form.app_key) {
        let _ =
            user_app_visibility::set_visibility(&state.pool, user_id, &form.app_key, visible).await;
    }

    // Return edit mode view so user can keep toggling
    Html(edit_fragment(&state, user_id, lang).await)
}

/// Whether `key` names an internal or external app on this deployment.
fn is_known_key(state: &AppState, key: &str) -> bool {
    state.apps.iter().any(|a| a.info().key == key)
        || state.config.external_apps.iter().any(|e| e.key == key)
}

async fn edit_fragment(state: &AppState, user_id: i64, lang: Lang) -> String {
    let base = &state.config.base_path;
    let visibility = user_app_visibility::get_visibility(&state.pool, user_id).await;
    let order = user_app_order::get_order(&state.pool, user_id).await;
    let entries = ordered_entries(&state.apps, &state.config.external_apps, &order);

    let header = render_header_edit(base, lang);
    let grid = render_grid_edit(&entries, &visibility, base, lang);
    format!("{header}{grid}")
}

#[derive(Deserialize)]
struct OrderForm {
    /// Comma-separated app keys, first card first.
    order: String,
}

/// Save the order the cards were dragged into. Unknown and repeated keys are
/// dropped; apps left out follow the saved ones.
async fn set_order(
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Form(form): Form<OrderForm>,
) -> StatusCode {
    let mut keys: Vec<&str> = Vec::new();
    for key in form.order.split(',') {
        if is_known_key(&state, key) && !keys.contains(&key) {
            keys.push(key);
        }
    }
    match user_app_order::set_order(&state.pool, user_id, &keys).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("Saving launcher order failed: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

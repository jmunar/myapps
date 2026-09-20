use axum::{Extension, Router, response::Html, routing::get};
use serde::Serialize;

/// Force-directed mind map. Kept in its own file so the Rust side never has
/// to brace-escape a page of JavaScript.
const MIND_MAP_JS: &str = include_str!("../static/mind-map.js");

use super::mindflow_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(page))
        .route("/map-data", get(map_data))
}

// -- Mind map page ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct CategoryOption {
    id: i64,
    name: String,
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let sv = &state.config.static_version;
    let t = super::i18n::t(lang);

    let categories: Vec<CategoryOption> = sqlx::query_as(
        "SELECT id, name FROM mindflow_categories WHERE user_id = ? AND archived = 0 ORDER BY name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let mut cat_options = format!(r#"<option value="">{}</option>"#, t.map_inbox_uncategorized);
    for c in &categories {
        cat_options.push_str(&format!(r#"<option value="{}">{}</option>"#, c.id, c.name,));
    }

    let inbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mindflow_thoughts WHERE user_id = ? AND category_id IS NULL AND status = 'active'",
    )
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let pending_actions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mindflow_actions WHERE user_id = ? AND status = 'pending'",
    )
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let inbox_badge = if inbox_count > 0 {
        format!(
            r#"<span class="badge badge-warning">{inbox_count} {}</span>"#,
            t.map_in_inbox
        )
    } else {
        String::new()
    };

    let actions_badge = if pending_actions > 0 {
        format!(
            r#"<span class="badge badge-info">{pending_actions} {}</span>"#,
            t.map_pending
        )
    } else {
        String::new()
    };

    let map_title = t.map_title;
    let map_subtitle = t.map_subtitle;
    let capture_placeholder = t.map_capture_placeholder;
    let capture_btn = t.map_capture;
    let first_thought = t.map_first_thought;

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>{map_title}</h1>
                <div>{inbox_badge} {actions_badge}</div>
            </div>
            <p>{map_subtitle}</p>
        </div>

        <div class="card">
            <div class="card-body">
                <form method="POST" action="{base}/mindflow/capture"
                      class="capture-form"
                      hx-post="{base}/mindflow/capture"
                      hx-target="#capture-feedback"
                      hx-swap="innerHTML"
                      hx-on::after-request="if(event.detail.successful){{this.reset();refreshMap()}}">
                    <input type="text" name="content" placeholder="{capture_placeholder}" required
                           class="capture-input" autocomplete="off">
                    <select name="category_id" class="capture-select">
                        {cat_options}
                    </select>
                    <button type="submit" class="btn btn-primary">{capture_btn}</button>
                </form>
                <div id="capture-feedback"></div>
            </div>
        </div>

        <div class="card mt-2">
            <div id="mindmap" class="mindmap-container" data-empty-label="{first_thought}"></div>
        </div>

        <script src="{base}/static/d3.v7.min.js?v={sv}"></script>
        <script>{MIND_MAP_JS}</script>"##,
    );

    Html(render_page(
        &format!("MindFlow \u{2014} {}", t.mind_map),
        &mindflow_nav(base, "map", lang),
        &body,
        &state.config,
        lang,
    ))
}

// -- Map data JSON endpoint ──────────────────────────────────

#[derive(Serialize)]
struct MapData {
    nodes: Vec<MapNode>,
    links: Vec<MapLink>,
}

#[derive(Serialize)]
struct MapNode {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    name: String,
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought_id: Option<i64>,
}

#[derive(Serialize)]
struct MapLink {
    source: String,
    target: String,
}

#[derive(sqlx::FromRow)]
struct CategoryForMap {
    id: i64,
    name: String,
    color: String,
    parent_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ThoughtForMap {
    id: i64,
    category_id: Option<i64>,
    parent_thought_id: Option<i64>,
    content: String,
}

async fn map_data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> axum::Json<MapData> {
    let categories: Vec<CategoryForMap> = sqlx::query_as(
        "SELECT id, name, color, parent_id FROM mindflow_categories WHERE user_id = ? AND archived = 0",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let thoughts: Vec<ThoughtForMap> = sqlx::query_as(
        "SELECT id, category_id, parent_thought_id, content FROM mindflow_thoughts WHERE user_id = ? AND status = 'active'",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let mut nodes = Vec::new();
    let mut links = Vec::new();

    let has_inbox = thoughts.iter().any(|t| t.category_id.is_none());

    // Add inbox virtual node if there are uncategorized thoughts
    if has_inbox {
        nodes.push(MapNode {
            id: "inbox".into(),
            node_type: "category".into(),
            name: "Inbox".into(),
            color: Some("#9E9E9E".into()),
            thought_id: None,
        });
    }

    // Category nodes
    for c in &categories {
        nodes.push(MapNode {
            id: format!("cat_{}", c.id),
            node_type: "category".into(),
            name: c.name.clone(),
            color: Some(c.color.clone()),
            thought_id: None,
        });

        // Link sub-categories to parent
        if let Some(parent_id) = c.parent_id {
            links.push(MapLink {
                source: format!("cat_{parent_id}"),
                target: format!("cat_{}", c.id),
            });
        }
    }

    // Thought nodes
    for t in &thoughts {
        let truncated: String = t.content.chars().take(40).collect();
        let cat_color = t
            .category_id
            .and_then(|cid| categories.iter().find(|c| c.id == cid))
            .map(|c| c.color.clone());

        nodes.push(MapNode {
            id: format!("t_{}", t.id),
            node_type: "thought".into(),
            name: truncated,
            color: cat_color,
            thought_id: Some(t.id),
        });

        // Link to parent thought if nested, otherwise to category/inbox
        let parent = if let Some(pid) = t.parent_thought_id {
            format!("t_{pid}")
        } else {
            match t.category_id {
                Some(cid) => format!("cat_{cid}"),
                None => "inbox".into(),
            }
        };
        links.push(MapLink {
            source: parent,
            target: format!("t_{}", t.id),
        });
    }

    axum::Json(MapData { nodes, links })
}

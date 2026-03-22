use axum::{Extension, Router, response::Html, routing::get};
use serde::Serialize;

use super::mindflow_nav;
use crate::auth::UserId;
use crate::i18n::Lang;
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(page))
        .route("/map-data", get(map_data))
}

// -- Mind map page ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct CategoryOption {
    id: i64,
    name: String,
    color: String,
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let categories: Vec<CategoryOption> = sqlx::query_as(
        "SELECT id, name, color FROM mindflow_categories WHERE user_id = ? AND archived = 0 ORDER BY name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut cat_options = format!(
        r#"<option value="">{}</option>"#,
        t.mf_map_inbox_uncategorized
    );
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
            t.mf_map_in_inbox
        )
    } else {
        String::new()
    };

    let actions_badge = if pending_actions > 0 {
        format!(
            r#"<span class="badge badge-info">{pending_actions} {}</span>"#,
            t.mf_map_pending
        )
    } else {
        String::new()
    };

    let map_title = t.mf_map_title;
    let map_subtitle = t.mf_map_subtitle;
    let capture_placeholder = t.mf_map_capture_placeholder;
    let capture_btn = t.mf_map_capture;
    let first_thought = t.mf_map_first_thought;

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
            <div id="mindmap" class="mindmap-container"></div>
        </div>

        <script src="{base}/static/d3.v7.min.js"></script>
        <script>
        (function() {{
            var basePath = '{base}';
            var width, height;
            var svg, simulation;

            function initMap() {{
                var container = document.getElementById('mindmap');
                width = container.clientWidth || 800;
                height = 500;

                container.innerHTML = '';
                svg = d3.select('#mindmap')
                    .append('svg')
                    .attr('width', width)
                    .attr('height', height)
                    .attr('viewBox', [0, 0, width, height]);

                window._mapG = svg.append('g');

                // Add zoom + pan behavior
                var zoom = d3.zoom()
                    .scaleExtent([0.3, 3])
                    .on('zoom', function(event) {{
                        window._mapG.attr('transform', event.transform);
                    }});
                svg.call(zoom);
            }}

            window.refreshMap = function() {{
                fetch(basePath + '/mindflow/map-data')
                    .then(function(r) {{ return r.json(); }})
                    .then(function(data) {{ renderGraph(data); }});
            }};

            function renderGraph(data) {{
                var g = window._mapG;
                g.selectAll('*').remove();

                if (data.nodes.length === 0) {{
                    svg.append('text')
                        .attr('x', width / 2)
                        .attr('y', height / 2)
                        .attr('text-anchor', 'middle')
                        .attr('fill', 'var(--text-secondary)')
                        .text('{first_thought}');
                    return;
                }}

                simulation = d3.forceSimulation(data.nodes)
                    .force('link', d3.forceLink(data.links).id(function(d) {{ return d.id; }}).distance(55))
                    .force('charge', d3.forceManyBody().strength(-100))
                    .force('center', d3.forceCenter(width / 2, height / 2))
                    .force('collision', d3.forceCollide().radius(function(d) {{
                        return d.type === 'category' ? 35 : 22;
                    }}));

                var link = g.append('g')
                    .selectAll('line')
                    .data(data.links)
                    .join('line')
                    .attr('stroke', '#ccc')
                    .attr('stroke-width', 1.5);

                var node = g.append('g')
                    .selectAll('g')
                    .data(data.nodes)
                    .join('g')
                    .call(d3.drag()
                        .on('start', function(event, d) {{
                            if (!event.active) simulation.alphaTarget(0.3).restart();
                            d.fx = d.x; d.fy = d.y;
                        }})
                        .on('drag', function(event, d) {{
                            d.fx = event.x; d.fy = event.y;
                        }})
                        .on('end', function(event, d) {{
                            if (!event.active) simulation.alphaTarget(0);
                            d.fx = null; d.fy = null;
                        }}));

                // Category nodes: larger circles
                node.filter(function(d) {{ return d.type === 'category'; }})
                    .append('circle')
                    .attr('r', 30)
                    .attr('fill', function(d) {{ return d.color || '#6B6B6B'; }})
                    .attr('opacity', 0.8)
                    .attr('stroke', '#fff')
                    .attr('stroke-width', 2);

                node.filter(function(d) {{ return d.type === 'category'; }})
                    .append('text')
                    .text(function(d) {{ return d.name; }})
                    .attr('text-anchor', 'middle')
                    .attr('dy', '0.35em')
                    .attr('fill', '#fff')
                    .attr('font-size', '11px')
                    .attr('font-weight', '600')
                    .style('pointer-events', 'none');

                // Thought nodes: smaller circles, clickable
                var thoughtNodes = node.filter(function(d) {{ return d.type === 'thought'; }});

                thoughtNodes.append('circle')
                    .attr('r', 14)
                    .attr('fill', function(d) {{ return d.color || '#ddd'; }})
                    .attr('opacity', 0.9)
                    .attr('stroke', '#fff')
                    .attr('stroke-width', 1)
                    .style('cursor', 'pointer');

                // Always-visible short labels next to thought nodes
                thoughtNodes.append('text')
                    .text(function(d) {{
                        var s = d.name;
                        return s.length > 20 ? s.substring(0, 18) + '...' : s;
                    }})
                    .attr('dx', 18)
                    .attr('dy', '0.35em')
                    .attr('fill', 'var(--text-secondary)')
                    .attr('font-size', '9px')
                    .attr('font-family', 'var(--font-body)')
                    .style('pointer-events', 'none')
                    .attr('class', 'thought-label');

                // Tap-to-select on touch, click-to-navigate on desktop
                var selectedNode = null;

                thoughtNodes.on('click', function(event, d) {{
                    var isTouch = 'ontouchstart' in window;
                    if (isTouch && selectedNode !== d.id) {{
                        // First tap: select and highlight
                        event.stopPropagation();
                        selectedNode = d.id;
                        // Reset all thought circles
                        thoughtNodes.select('circle')
                            .attr('stroke', '#fff')
                            .attr('stroke-width', 1)
                            .attr('r', 14);
                        // Highlight selected
                        d3.select(this).select('circle')
                            .attr('stroke', 'var(--accent)')
                            .attr('stroke-width', 3)
                            .attr('r', 18);
                        // Show full text in tooltip
                        tooltip.text(d.name)
                            .style('left', '1rem')
                            .style('bottom', '1rem')
                            .style('top', 'auto')
                            .style('opacity', 1);
                    }} else {{
                        // Second tap or desktop click: navigate
                        window.location.href = basePath + '/mindflow/thoughts/' + d.thought_id;
                    }}
                }});

                // Tap empty area to deselect on touch
                svg.on('click', function() {{
                    if (selectedNode) {{
                        selectedNode = null;
                        thoughtNodes.select('circle')
                            .attr('stroke', '#fff')
                            .attr('stroke-width', 1)
                            .attr('r', 14);
                        tooltip.style('opacity', 0);
                    }}
                }});

                // Desktop tooltip (instant hover)
                var tooltip = d3.select('#mindmap')
                    .append('div')
                    .attr('class', 'map-tooltip')
                    .style('opacity', 0);

                node.on('mouseenter', function(event, d) {{
                        if ('ontouchstart' in window) return;
                        tooltip.text(d.name)
                            .style('left', (event.offsetX + 12) + 'px')
                            .style('top', (event.offsetY - 8) + 'px')
                            .style('bottom', 'auto')
                            .style('opacity', 1);
                    }})
                    .on('mousemove', function(event) {{
                        if ('ontouchstart' in window) return;
                        tooltip
                            .style('left', (event.offsetX + 12) + 'px')
                            .style('top', (event.offsetY - 8) + 'px');
                    }})
                    .on('mouseleave', function() {{
                        if ('ontouchstart' in window) return;
                        tooltip.style('opacity', 0);
                    }});

                simulation.on('tick', function() {{
                    link.attr('x1', function(d) {{ return d.source.x; }})
                        .attr('y1', function(d) {{ return d.source.y; }})
                        .attr('x2', function(d) {{ return d.target.x; }})
                        .attr('y2', function(d) {{ return d.target.y; }});
                    node.attr('transform', function(d) {{
                        return 'translate(' + d.x + ',' + d.y + ')';
                    }});
                }});
            }}

            initMap();
            refreshMap();
        }})();
        </script>"##,
    );

    Html(render_page(
        &format!("MindFlow \u{2014} {}", t.mf_mind_map),
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
    .unwrap_or_default();

    let thoughts: Vec<ThoughtForMap> = sqlx::query_as(
        "SELECT id, category_id, parent_thought_id, content FROM mindflow_thoughts WHERE user_id = ? AND status = 'active'",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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

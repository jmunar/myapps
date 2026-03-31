//! Reusable UI components rendered as HTML strings.

/// Render a `serde_json::Value` as a collapsible HTML tree.
///
/// Null values, empty arrays, and empty objects are stripped.
/// Objects and arrays render as `<details>` / `<summary>` blocks;
/// scalars render inline.
pub fn render_json_viewer(value: &serde_json::Value) -> String {
    let cleaned = strip_nulls(value);
    let Some(cleaned) = cleaned else {
        return r#"<div class="json-viewer"><span class="json-null">null</span></div>"#.into();
    };
    let mut html = String::from(r#"<div class="json-viewer">"#);
    render_value(&cleaned, &mut html, true);
    html.push_str("</div>");
    html
}

fn strip_nulls(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Array(arr) => {
            let cleaned: Vec<serde_json::Value> = arr.iter().filter_map(strip_nulls).collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(serde_json::Value::Array(cleaned))
            }
        }
        serde_json::Value::Object(map) => {
            let cleaned: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter_map(|(k, v)| strip_nulls(v).map(|v| (k.clone(), v)))
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(cleaned))
            }
        }
        other => Some(other.clone()),
    }
}

fn render_value(value: &serde_json::Value, html: &mut String, open: bool) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                render_entry(key, val, html, open);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                render_entry(&format!("[{i}]"), val, html, open);
            }
        }
        _ => {
            render_scalar(value, html);
        }
    }
}

fn render_entry(key: &str, value: &serde_json::Value, html: &mut String, open: bool) {
    let escaped_key = html_escape(key);
    match value {
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            let open_attr = if open { " open" } else { "" };
            html.push_str(&format!(
                r#"<details class="json-node"{open_attr}><summary class="json-key">{escaped_key}</summary><div class="json-children">"#
            ));
            render_value(value, html, open);
            html.push_str("</div></details>");
        }
        _ => {
            html.push_str(&format!(
                r#"<div class="json-leaf"><span class="json-key">{escaped_key}</span>: "#
            ));
            render_scalar(value, html);
            html.push_str("</div>");
        }
    }
}

fn render_scalar(value: &serde_json::Value, html: &mut String) {
    match value {
        serde_json::Value::String(s) => {
            html.push_str(&format!(
                r#"<span class="json-string">"{}"</span>"#,
                html_escape(s)
            ));
        }
        serde_json::Value::Number(n) => {
            html.push_str(&format!(r#"<span class="json-number">{n}</span>"#));
        }
        serde_json::Value::Bool(b) => {
            html.push_str(&format!(r#"<span class="json-bool">{b}</span>"#));
        }
        serde_json::Value::Null => {
            html.push_str(r#"<span class="json-null">null</span>"#);
        }
        _ => {}
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

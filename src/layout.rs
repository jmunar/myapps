/// A single nav item for the shared layout.
pub struct NavItem {
    pub href: String,
    pub label: &'static str,
    pub active: bool,
}

/// Render a full HTML page shell with nav and body content.
pub fn render_page(title: &str, nav_items: &[NavItem], body_html: &str, base_path: &str) -> String {
    let mut nav_html = String::new();
    for item in nav_items {
        let active = if item.active { " class=\"active\"" } else { "" };
        // "Log out" is always right-aligned
        if item.label == "Log out" {
            nav_html.push_str(&format!(
                r#"<a href="{href}" class="nav-right">Log out</a>"#,
                href = item.href,
            ));
        } else {
            nav_html.push_str(&format!(
                r#"<a href="{href}"{active}>{label}</a>"#,
                href = item.href,
                label = item.label,
            ));
        }
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <link rel="stylesheet" href="{base_path}/static/style.css">
    <script src="{base_path}/static/htmx.min.js"></script>
</head>
<body>
    <nav>
        <a href="{base_path}/" class="brand">MyApps</a>
        {nav_html}
        </nav>
    <main>
        {body_html}
    </main>
</body>
</html>"#
    )
}

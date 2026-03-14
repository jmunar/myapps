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
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base_path}/static/style.css">
    <link rel="manifest" href="{base_path}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base_path}/static/icon.svg">
    <link rel="apple-touch-icon" href="{base_path}/static/icon.svg">
    <script src="{base_path}/static/htmx.min.js"></script>
    <script src="{base_path}/static/frappe-charts.min.umd.js"></script>
</head>
<body>
    <script>
    if ("serviceWorker" in navigator) {{
        navigator.serviceWorker.register("{base_path}/sw.js", {{ scope: "{base_path}/" }})
            .then(function(reg) {{
                if (!("PushManager" in window)) return;
                if (Notification.permission !== "granted") return;
                reg.pushManager.getSubscription().then(function(sub) {{
                    if (sub) return;
                    fetch("{base_path}/push/vapid-key").then(function(r) {{ return r.text(); }}).then(function(key) {{
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
                        fetch("{base_path}/push/subscribe", {{
                            method: "POST",
                            headers: {{ "Content-Type": "application/json" }},
                            body: JSON.stringify(body)
                        }});
                    }});
                }});
            }});
    }}
    </script>
    <nav>
        <a href="{base_path}/" class="brand">MyApps</a>
        {nav_html}
        </nav>
    <main>
        {body_html}
    </main>
</body>
</html>"##
    )
}

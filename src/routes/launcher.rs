use axum::{Router, response::Html, routing::get};

use super::AppState;
use crate::layout::{NavItem, render_page};

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

async fn index(state: axum::extract::State<AppState>) -> Html<String> {
    let base = &state.config.base_path;
    let nav = vec![NavItem {
        href: format!("{base}/logout"),
        label: "Log out",
        active: false,
    }];
    let body = format!(
        r#"<div class="page-header">
            <h1>My Apps</h1>
            <p>Choose an application</p>
        </div>
        <div class="launcher-grid">
            <a href="{base}/leanfin" class="launcher-card">
                <div class="launcher-icon">$</div>
                <div class="launcher-info">
                    <h2>LeanFin</h2>
                    <p>Personal expense tracker</p>
                </div>
            </a>
            <a href="{base}/mindflow" class="launcher-card">
                <div class="launcher-icon">&#x1F9E0;</div>
                <div class="launcher-info">
                    <h2>MindFlow</h2>
                    <p>Thought capture &amp; mind map</p>
                </div>
            </a>
            <a href="{base}/voice" class="launcher-card">
                <div class="launcher-icon">&#127908;</div>
                <div class="launcher-info">
                    <h2>VoiceToText</h2>
                    <p>Audio transcription with Whisper</p>
                </div>
            </a>
            <a href="{base}/classroom" class="launcher-card">
                <div class="launcher-icon">&#9998;</div>
                <div class="launcher-info">
                    <h2>ClassroomInput</h2>
                    <p>Record marks &amp; notes for classrooms</p>
                </div>
            </a>
        </div>
        <div id="push-status" style="text-align:center;margin-top:1.5rem;font-size:0.9rem;color:#888;"></div>
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
                            // Trigger subscription via SW registration
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
    );
    Html(render_page("MyApps", &nav, &body, base))
}

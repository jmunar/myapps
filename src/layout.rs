use crate::config::Config;
use crate::i18n::{self, Lang};

/// A single nav item for the shared layout.
pub struct NavItem {
    pub href: String,
    pub label: String,
    pub active: bool,
    /// If true, this item is rendered right-aligned (e.g. "Log out").
    pub right: bool,
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
<div id="cmd-window" class="cmd-window" style="display:none">
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
<script>
(function() {{
    var state = 'idle';
    var mediaRecorder, audioChunks = [], audioStream = null;
    var abortController = null;
    var discarded = false;
    var startX = 0;
    var SWIPE_THRESHOLD = 80;

    var mic = document.getElementById('cmd-mic');
    var swipeHint = document.getElementById('cmd-swipe-hint');
    var win = document.getElementById('cmd-window');
    var statusEl = document.getElementById('cmd-status');
    var transcriptionDiv = document.getElementById('cmd-transcription');
    var textSpan = document.getElementById('cmd-text');
    var editBtn = document.getElementById('cmd-edit-btn');
    var editArea = document.getElementById('cmd-edit-area');
    var editInput = document.getElementById('cmd-edit-input');
    var editDone = document.getElementById('cmd-edit-done');
    var resultEl = document.getElementById('command-result');
    var closeBtn = document.getElementById('cmd-close');
    var BASE = '{base_path}';
    var T_TRANSCRIBING = '{cmd_transcribing}';
    var T_INTERPRETING = '{cmd_interpreting}';
    var T_MIC_ERR = '{cmd_mic_no_permission}';

    mic.addEventListener('pointerdown', onPointerDown);
    mic.addEventListener('touchstart', function(e) {{ e.preventDefault(); }}, {{ passive: false }});
    closeBtn.addEventListener('click', dismiss);
    editBtn.addEventListener('click', startEdit);
    editDone.addEventListener('click', finishEdit);

    function onPointerDown(e) {{
        if (state !== 'idle') return;
        e.preventDefault();
        mic.setPointerCapture(e.pointerId);
        startX = e.clientX;
        discarded = false;
        startRecording(e.pointerId);
    }}

    function onPointerMove(e) {{
        if (state !== 'recording') return;
        var dx = startX - e.clientX;
        if (dx > 20) {{
            swipeHint.classList.add('visible');
            var pct = Math.min(dx / SWIPE_THRESHOLD, 1);
            swipeHint.style.opacity = pct;
            mic.style.transform = 'translateX(' + (-dx * 0.5) + 'px)';
        }} else {{
            swipeHint.classList.remove('visible');
            swipeHint.style.opacity = 0;
            mic.style.transform = '';
        }}
        if (dx >= SWIPE_THRESHOLD && !discarded) {{
            discarded = true;
            mic.classList.add('discarding');
            stopRecordingRaw();
        }}
    }}

    function onPointerUp(e) {{
        mic.releasePointerCapture(e.pointerId);
        mic.removeEventListener('pointermove', onPointerMove);
        mic.removeEventListener('pointerup', onPointerUp);
        mic.style.transform = '';
        swipeHint.classList.remove('visible');
        swipeHint.style.opacity = 0;
        mic.classList.remove('discarding');
        if (state === 'recording' && !discarded) {{
            stopRecording();
        }}
    }}

    function startRecording(pointerId) {{
        navigator.mediaDevices.getUserMedia({{ audio: true }}).then(function(stream) {{
            audioStream = stream;
            mediaRecorder = new MediaRecorder(stream);
            audioChunks = [];
            mediaRecorder.ondataavailable = function(e) {{ audioChunks.push(e.data); }};
            mediaRecorder.onstop = function() {{
                stream.getTracks().forEach(function(t) {{ t.stop(); }});
                audioStream = null;
                if (!discarded) {{
                    showWindow();
                    doTranscribe();
                }} else {{
                    resetToIdle();
                }}
            }};
            mediaRecorder.start();
            state = 'recording';
            mic.classList.add('recording');
            mic.addEventListener('pointermove', onPointerMove);
            mic.addEventListener('pointerup', onPointerUp);
        }}).catch(function() {{
            showError(T_MIC_ERR);
        }});
    }}

    function stopRecordingRaw() {{
        if (mediaRecorder && mediaRecorder.state === 'recording') {{
            mediaRecorder.stop();
        }}
        mic.classList.remove('recording');
    }}

    function stopRecording() {{
        stopRecordingRaw();
        state = 'transcribing';
    }}

    function showWindow() {{
        win.style.display = 'block';
        statusEl.textContent = T_TRANSCRIBING;
        statusEl.style.display = 'block';
        transcriptionDiv.style.display = 'none';
        editArea.style.display = 'none';
        resultEl.innerHTML = '';
    }}

    function doTranscribe() {{
        var blob = new Blob(audioChunks, {{ type: 'audio/webm' }});
        var form = new FormData();
        form.append('audio', blob, 'command.webm');
        fetch(BASE + '/command/transcribe', {{ method: 'POST', body: form }}).then(function(r) {{
            if (!r.ok) return r.text().then(function(t) {{ throw new Error(t); }});
            return r.text();
        }}).then(function(text) {{
            textSpan.textContent = text;
            transcriptionDiv.style.display = 'flex';
            statusEl.style.display = 'none';
            doInterpret(text);
        }}).catch(function(e) {{
            showError(e.message);
        }});
    }}

    function doInterpret(text) {{
        state = 'interpreting';
        statusEl.textContent = T_INTERPRETING;
        statusEl.style.display = 'block';
        abortController = new AbortController();
        fetch(BASE + '/command/interpret', {{
            method: 'POST',
            body: new URLSearchParams({{ input: text }}),
            signal: abortController.signal,
            headers: {{ 'Content-Type': 'application/x-www-form-urlencoded' }}
        }}).then(function(r) {{ return r.text(); }}).then(function(html) {{
            state = 'confirming';
            statusEl.style.display = 'none';
            resultEl.innerHTML = html;
            htmx.process(resultEl);
            wireResultButtons();
        }}).catch(function(e) {{
            if (e.name === 'AbortError') return;
            showError(e.message);
        }});
    }}

    function startEdit() {{
        if (abortController) abortController.abort();
        state = 'editing';
        editInput.value = textSpan.textContent;
        editArea.style.display = 'block';
        transcriptionDiv.style.display = 'none';
        statusEl.style.display = 'none';
        resultEl.innerHTML = '';
        editInput.focus();
    }}

    function finishEdit() {{
        var text = editInput.value.trim();
        if (!text) return;
        textSpan.textContent = text;
        editArea.style.display = 'none';
        transcriptionDiv.style.display = 'flex';
        doInterpret(text);
    }}

    function wireResultButtons() {{
        var cb = resultEl.querySelector('.cmd-cancel-btn');
        if (cb) cb.onclick = dismiss;
        resultEl.addEventListener('htmx:afterSwap', function() {{
            if (resultEl.querySelector('.command-success') || resultEl.querySelector('script')) {{
                setTimeout(dismiss, 2000);
            }}
        }});
    }}

    function dismiss() {{
        if (abortController) abortController.abort();
        if (mediaRecorder && mediaRecorder.state === 'recording') {{
            mediaRecorder.stop();
        }}
        resetToIdle();
        win.style.display = 'none';
        resultEl.innerHTML = '';
    }}

    function resetToIdle() {{
        state = 'idle';
        mic.classList.remove('recording');
        mic.classList.remove('discarding');
        mic.style.transform = '';
    }}

    function showError(msg) {{
        resetToIdle();
        statusEl.style.display = 'none';
        transcriptionDiv.style.display = 'none';
        editArea.style.display = 'none';
        resultEl.innerHTML = '<div class="command-error">' + msg + '</div>';
        win.style.display = 'block';
    }}
}})();
</script>"##,
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
<html lang="{lang_code}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base_path}/static/style.css?v={sv}">
    <link rel="manifest" href="{base_path}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base_path}/static/icon.svg?v={sv}">
    <link rel="apple-touch-icon" href="{base_path}/static/icon.svg?v={sv}">
    <script src="{base_path}/static/htmx.min.js?v={sv}"></script>
    <script src="{base_path}/static/frappe-charts.min.umd.js?v={sv}"></script>
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
    {command_bar}
</body>
</html>"##
    )
}

// Voice command bar: hold the mic to record, swipe left to discard, then
// confirm the interpreted action. Inlined by layout.rs on every page where the
// LLM and whisper are both configured.
//
// The base path comes from <html data-base> and the three status strings from
// data attributes on #cmd-window, so this file is the same bytes in every
// language and needs nothing interpolated into it.

(function() {
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
    var BASE = document.documentElement.dataset.base || '';
    var T_TRANSCRIBING = win.dataset.transcribing;
    var T_INTERPRETING = win.dataset.interpreting;
    var T_MIC_ERR = win.dataset.micError;

    mic.addEventListener('pointerdown', onPointerDown);
    mic.addEventListener('touchstart', function(e) { e.preventDefault(); }, { passive: false });
    closeBtn.addEventListener('click', dismiss);
    editBtn.addEventListener('click', startEdit);
    editDone.addEventListener('click', finishEdit);

    function onPointerDown(e) {
        if (state !== 'idle') return;
        e.preventDefault();
        mic.setPointerCapture(e.pointerId);
        startX = e.clientX;
        discarded = false;
        startRecording(e.pointerId);
    }

    function onPointerMove(e) {
        if (state !== 'recording') return;
        var dx = startX - e.clientX;
        if (dx > 20) {
            swipeHint.classList.add('visible');
            var pct = Math.min(dx / SWIPE_THRESHOLD, 1);
            swipeHint.style.opacity = pct;
            mic.style.transform = 'translateX(' + (-dx * 0.5) + 'px)';
        } else {
            swipeHint.classList.remove('visible');
            swipeHint.style.opacity = 0;
            mic.style.transform = '';
        }
        if (dx >= SWIPE_THRESHOLD && !discarded) {
            discarded = true;
            mic.classList.add('discarding');
            stopRecordingRaw();
        }
    }

    function onPointerUp(e) {
        mic.releasePointerCapture(e.pointerId);
        mic.removeEventListener('pointermove', onPointerMove);
        mic.removeEventListener('pointerup', onPointerUp);
        mic.style.transform = '';
        swipeHint.classList.remove('visible');
        swipeHint.style.opacity = 0;
        mic.classList.remove('discarding');
        if (state === 'recording' && !discarded) {
            stopRecording();
        }
    }

    function startRecording(pointerId) {
        navigator.mediaDevices.getUserMedia({ audio: true }).then(function(stream) {
            audioStream = stream;
            mediaRecorder = new MediaRecorder(stream);
            audioChunks = [];
            mediaRecorder.ondataavailable = function(e) { audioChunks.push(e.data); };
            mediaRecorder.onstop = function() {
                stream.getTracks().forEach(function(t) { t.stop(); });
                audioStream = null;
                if (!discarded) {
                    showWindow();
                    doTranscribe();
                } else {
                    resetToIdle();
                }
            };
            mediaRecorder.start();
            state = 'recording';
            mic.classList.add('recording');
            mic.addEventListener('pointermove', onPointerMove);
            mic.addEventListener('pointerup', onPointerUp);
        }).catch(function() {
            showError(T_MIC_ERR);
        });
    }

    function stopRecordingRaw() {
        if (mediaRecorder && mediaRecorder.state === 'recording') {
            mediaRecorder.stop();
        }
        mic.classList.remove('recording');
    }

    function stopRecording() {
        stopRecordingRaw();
        state = 'transcribing';
    }

    function showWindow() {
        win.style.display = 'block';
        statusEl.textContent = T_TRANSCRIBING;
        statusEl.style.display = 'block';
        transcriptionDiv.style.display = 'none';
        editArea.style.display = 'none';
        resultEl.innerHTML = '';
    }

    function doTranscribe() {
        var blob = new Blob(audioChunks, { type: 'audio/webm' });
        var form = new FormData();
        form.append('audio', blob, 'command.webm');
        fetch(BASE + '/command/transcribe', { method: 'POST', body: form }).then(function(r) {
            if (!r.ok) return r.text().then(function(t) { throw new Error(t); });
            return r.text();
        }).then(function(text) {
            textSpan.textContent = text;
            transcriptionDiv.style.display = 'flex';
            statusEl.style.display = 'none';
            doInterpret(text);
        }).catch(function(e) {
            showError(e.message);
        });
    }

    function doInterpret(text) {
        state = 'interpreting';
        statusEl.textContent = T_INTERPRETING;
        statusEl.style.display = 'block';
        abortController = new AbortController();
        fetch(BASE + '/command/interpret', {
            method: 'POST',
            body: new URLSearchParams({ input: text }),
            signal: abortController.signal,
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' }
        }).then(function(r) { return r.text(); }).then(function(html) {
            state = 'confirming';
            statusEl.style.display = 'none';
            resultEl.innerHTML = html;
            htmx.process(resultEl);
            wireResultButtons();
        }).catch(function(e) {
            if (e.name === 'AbortError') return;
            showError(e.message);
        });
    }

    function startEdit() {
        if (abortController) abortController.abort();
        state = 'editing';
        editInput.value = textSpan.textContent;
        editArea.style.display = 'block';
        transcriptionDiv.style.display = 'none';
        statusEl.style.display = 'none';
        resultEl.innerHTML = '';
        editInput.focus();
    }

    function finishEdit() {
        var text = editInput.value.trim();
        if (!text) return;
        textSpan.textContent = text;
        editArea.style.display = 'none';
        transcriptionDiv.style.display = 'flex';
        doInterpret(text);
    }

    function wireResultButtons() {
        var cb = resultEl.querySelector('.cmd-cancel-btn');
        if (cb) cb.onclick = dismiss;
        resultEl.addEventListener('htmx:afterSwap', function() {
            if (resultEl.querySelector('.command-success') || resultEl.querySelector('script')) {
                setTimeout(dismiss, 2000);
            }
        });
    }

    function dismiss() {
        if (abortController) abortController.abort();
        if (mediaRecorder && mediaRecorder.state === 'recording') {
            mediaRecorder.stop();
        }
        resetToIdle();
        win.style.display = 'none';
        resultEl.innerHTML = '';
    }

    function resetToIdle() {
        state = 'idle';
        mic.classList.remove('recording');
        mic.classList.remove('discarding');
        mic.style.transform = '';
    }

    function showError(msg) {
        resetToIdle();
        statusEl.style.display = 'none';
        transcriptionDiv.style.display = 'none';
        editArea.style.display = 'none';
        resultEl.innerHTML = '<div class="command-error">' + msg + '</div>';
        win.style.display = 'block';
    }
})();

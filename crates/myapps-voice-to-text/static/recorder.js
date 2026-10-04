// Microphone recorder on the "new job" page: record in the browser, POST the
// blob to /voice/upload, drop the returned fragment into #rec-result.
//
// The status strings come off #rec-status as data attributes and the base path
// off <html data-base>, so nothing is interpolated into this file.

(function () {
  var startBtn = document.getElementById('rec-start');
  var stopBtn = document.getElementById('rec-stop');
  var statusEl = document.getElementById('rec-status');
  if (!startBtn || !stopBtn || !statusEl) return;

  var base = document.documentElement.dataset.base || '';
  var mediaRecorder;
  var audioChunks = [];

  async function startRecording() {
    var stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    mediaRecorder = new MediaRecorder(stream);
    audioChunks = [];
    mediaRecorder.ondataavailable = function (e) {
      audioChunks.push(e.data);
    };
    mediaRecorder.onstop = async function () {
      stream.getTracks().forEach(function (t) {
        t.stop();
      });
      var blob = new Blob(audioChunks, { type: 'audio/webm' });
      var form = new FormData();
      form.append('audio', blob, 'recording.webm');
      form.append('model', document.getElementById('model').value);
      var resp = await fetch(base + '/voice/upload', {
        method: 'POST',
        body: form,
      });
      document.getElementById('rec-result').innerHTML = await resp.text();
    };
    mediaRecorder.start();
    startBtn.disabled = true;
    stopBtn.disabled = false;
    statusEl.textContent = statusEl.dataset.recording;
  }

  function stopRecording() {
    mediaRecorder.stop();
    startBtn.disabled = false;
    stopBtn.disabled = true;
    statusEl.textContent = statusEl.dataset.processing;
  }

  startBtn.addEventListener('click', startRecording);
  stopBtn.addEventListener('click', stopRecording);
})();

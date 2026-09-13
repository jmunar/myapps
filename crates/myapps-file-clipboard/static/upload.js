// FileClipboard page script: drag-and-drop uploader plus error-feedback wiring.
//
// Uses XMLHttpRequest rather than htmx or fetch(): neither reports upload
// progress, and a multi-gigabyte upload with no progress bar looks like a hang.
// Files upload one at a time so several large drops cannot saturate the
// server's disk and memory at once.
(function () {
  const zone = document.getElementById("fc-dropzone");
  if (!zone) return;

  const base = zone.dataset.base || "";
  const input = document.getElementById("fc-file-input");
  const browse = document.getElementById("fc-browse");
  const progress = document.getElementById("fc-progress");
  const status = document.getElementById("fc-upload-status");
  const list = document.getElementById("fc-file-list");

  const queue = [];
  let busy = false;

  function enqueue(files) {
    for (const file of files) queue.push(file);
    pump();
  }

  function pump() {
    if (busy) return;
    const file = queue.shift();
    if (!file) return;
    busy = true;
    uploadOne(file).then(() => {
      busy = false;
      pump();
    });
  }

  function uploadOne(file) {
    return new Promise((resolve) => {
      const row = document.createElement("div");
      row.className = "fc-progress-row";

      const name = document.createElement("span");
      name.className = "fc-progress-name";
      name.textContent = file.name;

      const bar = document.createElement("progress");
      bar.max = 100;
      bar.value = 0;

      row.appendChild(name);
      row.appendChild(bar);
      progress.appendChild(row);

      const body = new FormData();
      body.append("file", file, file.name);

      const xhr = new XMLHttpRequest();
      xhr.open("POST", base + "/file_clipboard/upload");

      xhr.upload.addEventListener("progress", (e) => {
        if (e.lengthComputable) bar.value = (e.loaded / e.total) * 100;
      });

      xhr.addEventListener("load", () => {
        row.remove();
        if (xhr.status >= 200 && xhr.status < 300) {
          status.innerHTML = "";
          if (list) list.innerHTML = xhr.responseText;
        } else {
          status.innerHTML = xhr.responseText || "";
        }
        resolve();
      });

      xhr.addEventListener("error", () => {
        row.remove();
        status.textContent = zone.dataset.errFailed || "Upload failed.";
        resolve();
      });

      xhr.send(body);
    });
  }

  // Without preventDefault on the window, dropping a file anywhere outside the
  // zone makes the browser navigate away from the page mid-upload.
  for (const type of ["dragover", "drop"]) {
    window.addEventListener(type, (e) => e.preventDefault());
  }

  for (const type of ["dragenter", "dragover"]) {
    zone.addEventListener(type, (e) => {
      e.preventDefault();
      zone.classList.add("fc-dragover");
    });
  }

  for (const type of ["dragleave", "dragend"]) {
    zone.addEventListener(type, () => zone.classList.remove("fc-dragover"));
  }

  zone.addEventListener("drop", (e) => {
    e.preventDefault();
    zone.classList.remove("fc-dragover");
    if (e.dataTransfer && e.dataTransfer.files.length) enqueue(e.dataTransfer.files);
  });

  zone.addEventListener("click", () => input.click());
  if (browse) {
    browse.addEventListener("click", (e) => {
      e.stopPropagation();
      input.click();
    });
  }

  input.addEventListener("change", () => {
    if (input.files.length) enqueue(input.files);
    input.value = "";
  });

  // htmx discards the body of a non-2xx response, so the 400 from an invalid
  // retention period would render nothing — leaving the previous "Saved."
  // message on screen, telling the user the opposite of what happened. Let
  // client-error bodies swap into their target.
  document.body.addEventListener("htmx:beforeSwap", (e) => {
    const status = e.detail.xhr.status;
    if (status >= 400 && status < 500) {
      e.detail.shouldSwap = true;
      e.detail.isError = false;
    }
  });

  // The browser blocks submission outright when the number input is out of
  // range, which would also strand a stale message. Clear it as soon as the
  // value changes.
  const retention = document.querySelector('input[name="retention_days"]');
  const settingsStatus = document.getElementById("fc-settings-status");
  if (retention && settingsStatus) {
    retention.addEventListener("input", () => {
      settingsStatus.innerHTML = "";
    });
  }
})();

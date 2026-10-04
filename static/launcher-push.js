// Notification status line on the launcher, plus the button that asks for
// permission. Labels arrive as data attributes on #push-status so this file
// needs nothing interpolated into it.

(function () {
  var el = document.getElementById('push-status');
  if (!el) return;
  if (!('Notification' in window) || !('PushManager' in window)) return;

  var t = el.dataset;

  if (Notification.permission === 'granted') {
    el.textContent = t.enabled;
    return;
  }
  if (Notification.permission === 'denied') {
    el.textContent = t.blockedSettings;
    return;
  }

  var btn = document.createElement('button');
  btn.textContent = t.enable;
  btn.className = 'btn btn-secondary';
  btn.addEventListener('click', function () {
    Notification.requestPermission().then(function (perm) {
      if (perm !== 'granted') {
        el.textContent = t.blocked;
        return;
      }
      el.textContent = t.enabled;
      navigator.serviceWorker.ready.then(function (reg) {
        return window.MyAppsPush.subscribe(reg);
      });
    });
  });
  el.appendChild(btn);
})();

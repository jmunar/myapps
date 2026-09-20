// Web-push subscription, shared by the service-worker bootstrap in layout.rs
// and the launcher's "enable notifications" button.
//
// Both used to carry their own copy of the VAPID key decode and the POST to
// /push/subscribe. This is that code, once. Inlined by layout.rs on every
// page, so `window.MyAppsPush` is available to any page script that needs it.

window.MyAppsPush = (function () {
  var base = document.documentElement.dataset.base || '';

  // VAPID keys arrive base64url; PushManager wants raw bytes.
  function urlBase64ToUint8Array(key) {
    var padding = (4 - (key.length % 4)) % 4;
    var b64 = key.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat(padding);
    var raw = atob(b64);
    var arr = new Uint8Array(raw.length);
    for (var i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
    return arr;
  }

  function b64url(buf) {
    return btoa(String.fromCharCode.apply(null, new Uint8Array(buf)))
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/, '');
  }

  // Subscribe `reg` to push and register it server-side. Resolves whether or
  // not a new subscription was created; rejects only on a genuine failure.
  function subscribe(reg) {
    return reg.pushManager.getSubscription().then(function (existing) {
      if (existing) return existing;
      return fetch(base + '/push/vapid-key')
        .then(function (r) {
          return r.text();
        })
        .then(function (key) {
          return reg.pushManager.subscribe({
            userVisibleOnly: true,
            applicationServerKey: urlBase64ToUint8Array(key),
          });
        })
        .then(function (sub) {
          if (!sub) return null;
          return fetch(base + '/push/subscribe', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              endpoint: sub.endpoint,
              p256dh: b64url(sub.getKey('p256dh')),
              auth: b64url(sub.getKey('auth')),
            }),
          }).then(function () {
            return sub;
          });
        });
    });
  }

  return { subscribe: subscribe, basePath: base };
})();

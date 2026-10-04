// Registers the service worker on every page, and — only if the user has
// already granted notifications — makes sure a push subscription exists.
//
// Asking for permission is the launcher's job (see launcher-push.js); this
// never prompts.

(function () {
  if (!('serviceWorker' in navigator)) return;
  var base = document.documentElement.dataset.base || '';

  navigator.serviceWorker
    .register(base + '/sw.js', { scope: base + '/' })
    .then(function (reg) {
      // The login page registers the worker but never inlines push.js —
      // there is no session to attach a subscription to yet.
      if (!window.MyAppsPush) return;
      if (!('PushManager' in window)) return;
      if (Notification.permission !== 'granted') return;
      return window.MyAppsPush.subscribe(reg);
    })
    .catch(function () {
      /* a failed registration must not take the page down with it */
    });
})();

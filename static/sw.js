// BASE_PATH and STATIC_VERSION are injected by the server before this file.
const CACHE_NAME = "myapps-" + STATIC_VERSION;

// Versioned URLs because every <link>/<script> in layout.rs appends ?v={sv};
// without the query string the pre-cached entries don't match what the
// browser actually requests.
const V = "?v=" + STATIC_VERSION;
const STATIC_ASSETS = [
  BASE_PATH + "/static/core.css" + V,
  BASE_PATH + "/static/apps.css" + V,
  BASE_PATH + "/static/htmx.min.js" + V,
  BASE_PATH + "/static/chart.min.js" + V,
  BASE_PATH + "/static/notes-vendor.bundle.js" + V,
  BASE_PATH + "/static/notes-tiptap-bootstrap.js" + V,
  BASE_PATH + "/static/icon.svg",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(STATIC_ASSETS))
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key !== CACHE_NAME)
          .map((key) => caches.delete(key))
      )
    )
  );
  self.clients.claim();
});

// Helper: store a successful response in the cache and return the original.
function cachePut(request, response) {
  if (!response || !response.ok || response.type === "opaque") return;
  const copy = response.clone();
  caches.open(CACHE_NAME).then((cache) => cache.put(request, copy));
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const url = new URL(request.url);

  // Cache-first + write-through for static assets so anything fetched
  // online is available offline next time.
  if (url.pathname.startsWith(BASE_PATH + "/static/")) {
    event.respondWith(
      caches.match(request).then((cached) => {
        if (cached) return cached;
        return fetch(request).then((response) => {
          cachePut(request, response);
          return response;
        });
      })
    );
    return;
  }

  // Network-first for HTML pages with write-through cache. Lets previously
  // visited pages (notably /notes/{id}/edit) load when offline.
  if (request.mode === "navigate" || request.headers.get("accept")?.includes("text/html")) {
    event.respondWith(
      fetch(request)
        .then((response) => {
          cachePut(request, response);
          return response;
        })
        .catch(() => caches.match(request))
    );
    return;
  }

  // Default: network with cache fallback (no write-through — this catches
  // POSTs of forms, fetch() to JSON endpoints, etc.)
  event.respondWith(
    fetch(request).catch(() => caches.match(request))
  );
});

self.addEventListener("push", (event) => {
  let data = { title: "MyApps", body: "" };
  try {
    data = event.data.json();
  } catch (e) {
    data.body = event.data ? event.data.text() : "";
  }
  event.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.body,
      icon: BASE_PATH + "/static/icon.svg",
    })
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(
    clients.matchAll({ type: "window", includeUncontrolled: true }).then((windowClients) => {
      for (const client of windowClients) {
        if (client.url.includes(BASE_PATH) && "focus" in client) {
          return client.focus();
        }
      }
      return clients.openWindow(BASE_PATH + "/");
    })
  );
});

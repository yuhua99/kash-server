const CACHE = "kash-v1";

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(["/", "/manifest.webmanifest"])));
});

self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key)));
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") {
    return;
  }

  const url = new URL(request.url);
  if (url.origin !== self.location.origin || url.pathname.startsWith("/api")) {
    return;
  }

  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);

      if (request.mode === "navigate") {
        try {
          const response = await fetch(request);
          if (response.ok) {
            event.waitUntil(cache.put(request, response.clone()));
          }
          return response;
        } catch (error) {
          const cached = (await cache.match(request)) || (await cache.match("/"));
          if (cached) {
            return cached;
          }
          throw error;
        }
      }

      const cached = await cache.match(request);
      if (cached) {
        event.waitUntil(
          fetch(request)
            .then((response) => {
              if (response.ok) {
                return cache.put(request, response.clone());
              }
            })
            .catch(() => {}),
        );
        return cached;
      }

      const response = await fetch(request);
      if (response.ok) {
        cache.put(request, response.clone());
      }
      return response;
    })(),
  );
});

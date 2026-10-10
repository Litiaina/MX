/* Office-only public-asset cache. NEVER caches tokens, API replies or documents. */
'use strict';
let config;
const configuration = () => config ||= (async () => {
  try {
    const response = await fetch('/office/precache.json', { cache: 'no-cache' });
    if (!response.ok) throw new Error('Office offline manifest unavailable');
    return await response.json();
  } catch (error) {
    // Activation may happen after the connection disappears or the worker
    // restarts. Only a fully installed cache has this completion marker.
    for (const key of (await caches.keys()).reverse()) {
      if (!key.startsWith('mx-office-assets-')) continue;
      const manifest = await (await caches.open(key)).match('/office/precache.json');
      if (manifest) return manifest.json();
    }
    throw error;
  }
})();
self.addEventListener('install', event => event.waitUntil((async () => {
  const manifest = await configuration();
  const cache = await caches.open(manifest.cache);
  // Sequential caching bounds decompression/memory pressure on weak devices.
  try {
    for (const path of manifest.assets) {
      const response = await fetch(path, { cache: 'reload' });
      if (!response.ok || response.type === 'opaque') throw new Error(`Office asset unavailable: ${path}`);
      await cache.put(path, response);
    }
    await cache.put('/office/precache.json', new Response(JSON.stringify(manifest), { headers: {'Content-Type':'application/json'} }));
  } catch (error) {
    if (!await cache.match('/office/precache.json')) await caches.delete(manifest.cache);
    throw error;
  }
  await self.skipWaiting();
})()));
self.addEventListener('activate', event => event.waitUntil((async () => {
  // Use the installed completion marker, not a possibly newer server manifest.
  // Otherwise a deploy between install and activation could delete the working
  // cache in favor of a generation this worker has never downloaded.
  let manifest;
  for (const key of (await caches.keys()).reverse()) {
    if (!key.startsWith('mx-office-assets-')) continue;
    const response = await (await caches.open(key)).match('/office/precache.json');
    if (response) { manifest = await response.json(); break; }
  }
  manifest ||= await configuration();
  for (const key of await caches.keys()) if (key.startsWith('mx-office-assets-') && key !== manifest.cache) await caches.delete(key);
  await self.clients.claim();
})()));
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== self.location.origin || (!url.pathname.startsWith('/office/') && !url.pathname.startsWith('/assets/'))) return;
  // The shell ignores file fragments. Strip navigation query strings as well.
  const path = url.pathname === '/office/' ? '/office/index.html' : url.pathname;
  event.respondWith((async () => {
    // Cache names are intentionally discovered without fetching a manifest:
    // the service worker must be able to start when the server is unreachable.
    const names = (await caches.keys()).filter(key => key.startsWith('mx-office-assets-'));
    for (const name of names.reverse()) {
      const cache = await caches.open(name);
      if (!await cache.match('/office/precache.json')) continue;
      const cached = await cache.match(path); if (cached) return cached;
    }
    return fetch(event.request);
  })());
});

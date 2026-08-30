// PWA service worker: keeps the Share Target flow intact and adds offline
// caching for the shell and thumbnails already seen.
//
// Intercepts the POST that the OS sends to `/share-target` when the user
// picks "Share -> Keeppix" from their phone's gallery, saves the received
// files in Cache Storage (the simplest channel for getting `File`s from the
// service worker to the page: it survives the redirect, unlike
// `postMessage`, which would require an already-open client), and sends the
// user back to the SPA via GET `/share-target`, where `ShareTargetView.vue`
// reads them back and hands them to the upload panel.
//
// Strategy:
// - POST /share-target: unchanged, intercepted and redirected to the SPA.
// - HTML navigation: network-first with fallback to the shell cache.
// - Vite's hashed static assets + manifest/favicon: cache-first.
// - /media/thumb/*: cache-first, so thumbnails already seen stay
//   navigable offline.
//
// NOTE: the cache name and the keys below must stay identical to the ones
// in `frontend/src/pwa/shareTarget.ts` — they can't be shared at build
// time because this file is served as-is from `/public`, without going
// through the bundler.
const SHARE_CACHE_NAME = 'keeppix-share-target-v1'
const SHARE_INDEX_KEY = '/__share-target-index__'
const SHELL_CACHE_NAME = 'keeppix-shell-v2'
const THUMB_CACHE_NAME = 'keeppix-thumbs-v1'
const APP_SHELL = ['/', '/manifest.webmanifest', '/favicon.svg', '/icon-192.png', '/icon-512.png']

self.addEventListener('install', (event) => {
  // We don't use `skipWaiting()`: a new service worker stays waiting until
  // the old tabs close, so it doesn't silently replace a shell while the
  // user has active operations.
  event.waitUntil(precacheShell())
})

self.addEventListener('activate', (event) => {
  event.waitUntil(cleanupCaches())
})

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url)
  if (event.request.method === 'POST' && url.pathname === '/share-target') {
    event.respondWith(handleShareTarget(event))
    return
  }
  if (event.request.method !== 'GET') {
    return
  }
  if (event.request.mode === 'navigate') {
    event.respondWith(navigationResponse(event.request))
    return
  }
  if (url.origin !== self.location.origin) {
    return
  }
  if (url.pathname.startsWith('/media/thumb/')) {
    event.respondWith(cacheFirst(THUMB_CACHE_NAME, event.request))
    return
  }
  if (isShellAsset(url.pathname)) {
    event.respondWith(cacheFirst(SHELL_CACHE_NAME, event.request))
  }
})

async function handleShareTarget(event) {
  try {
    const formData = await event.request.formData()
    const files = formData.getAll('files').filter((entry) => entry instanceof File)
    const cache = await caches.open(SHARE_CACHE_NAME)

    const index = files.map((file, i) => ({
      key: `/__share-target-file-${i}__`,
      name: file.name,
      type: file.type
    }))

    await Promise.all(
      files.map((file, i) =>
        cache.put(
          index[i].key,
          new Response(file, {
            headers: { 'content-type': file.type || 'application/octet-stream' }
          })
        )
      )
    )
    await cache.put(
      SHARE_INDEX_KEY,
      new Response(JSON.stringify(index), {
        headers: { 'content-type': 'application/json' }
      })
    )
  } catch {
    // A failure reading the FormData shouldn't prevent the redirect: the
    // SPA will simply find no pending files.
  }
  return Response.redirect('/share-target', 303)
}

async function precacheShell() {
  const cache = await caches.open(SHELL_CACHE_NAME)
  await cache.addAll(APP_SHELL)
}

async function cleanupCaches() {
  const keep = new Set([SHARE_CACHE_NAME, SHELL_CACHE_NAME, THUMB_CACHE_NAME])
  const names = await caches.keys()
  await Promise.all(names.filter((name) => !keep.has(name)).map((name) => caches.delete(name)))
  await self.clients.claim()
}

function isShellAsset(pathname) {
  return pathname === '/manifest.webmanifest' || pathname === '/favicon.svg' || pathname === '/icon-192.png' || pathname === '/icon-512.png' || pathname.startsWith('/assets/')
}

async function navigationResponse(request) {
  const cache = await caches.open(SHELL_CACHE_NAME)
  try {
    const response = await fetch(request)
    if (response.ok) {
      await cache.put('/', response.clone())
    }
    return response
  } catch {
    return (await cache.match('/')) || Response.error()
  }
}

async function cacheFirst(cacheName, request) {
  const cache = await caches.open(cacheName)
  const cached = await cache.match(request)
  if (cached) {
    return cached
  }
  const response = await fetch(request)
  if (response.ok) {
    await cache.put(request, response.clone())
  }
  return response
}

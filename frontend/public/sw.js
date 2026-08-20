// Service worker della PWA: mantiene intatto lo Share Target della Fase 5 e
// aggiunge caching offline per shell e miniature già viste.
//
// Intercetta il POST che il sistema operativo manda a `/share-target`
// quando l'utente sceglie "Condividi -> Keeppix" dalla galleria del
// telefono, salva i file ricevuti in Cache Storage (il canale più semplice
// per far arrivare dei `File` dal service worker alla pagina: sopravvive al
// redirect, a differenza di `postMessage` che richiederebbe un client già
// aperto) e rimanda l'utente alla SPA su GET `/share-target`, dove
// `ShareTargetView.vue` li rilegge e li passa al pannello di upload.
//
// Strategia:
// - POST /share-target: invariato, intercettato e rediretto alla SPA.
// - Navigazione HTML: network-first con fallback alla shell cache.
// - Asset statici hashed di Vite + manifest/favicon: cache-first.
// - /media/thumb/*: cache-first, così le miniature già viste restano
//   navigabili offline.
//
// NOTA: il nome della cache e le chiavi qui sotto devono restare identici a
// quelli in `frontend/src/pwa/shareTarget.ts` — non condivisibili a build
// time perché questo file è servito com'è da `/public`, senza passare dal
// bundler.
const SHARE_CACHE_NAME = 'keeppix-share-target-v1'
const SHARE_INDEX_KEY = '/__share-target-index__'
const SHELL_CACHE_NAME = 'keeppix-shell-v2'
const THUMB_CACHE_NAME = 'keeppix-thumbs-v1'
const APP_SHELL = ['/', '/manifest.webmanifest', '/favicon.svg', '/icon-192.png', '/icon-512.png']

self.addEventListener('install', (event) => {
  // Non usiamo `skipWaiting()`: un service worker nuovo resta in attesa finché
  // le tab vecchie non si chiudono, così non rimpiazza in silenzio una shell
  // mentre l'utente ha operazioni attive.
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
    // Un fallimento nella lettura del FormData non deve impedire il
    // redirect: la SPA troverà semplicemente nessun file pendente.
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

// Service worker minimo per la Web Share Target API (Fase 5, Task 10).
//
// Intercetta il POST che il sistema operativo manda a `/share-target`
// quando l'utente sceglie "Condividi -> Keeppix" dalla galleria del
// telefono, salva i file ricevuti in Cache Storage (il canale più semplice
// per far arrivare dei `File` dal service worker alla pagina: sopravvive al
// redirect, a differenza di `postMessage` che richiederebbe un client già
// aperto) e rimanda l'utente alla SPA su GET `/share-target`, dove
// `ShareTargetView.vue` li rilegge e li passa al pannello di upload.
//
// Nessun'altra responsabilità: non fa caching di asset, non intercetta
// altre richieste. Non è (ancora) un service worker offline-first.
//
// NOTA: il nome della cache e le chiavi qui sotto devono restare identici a
// quelli in `frontend/src/pwa/shareTarget.ts` — non condivisibili a build
// time perché questo file è servito com'è da `/public`, senza passare dal
// bundler.
const SHARE_CACHE_NAME = 'keeppix-share-target-v1'
const SHARE_INDEX_KEY = '/__share-target-index__'

self.addEventListener('install', () => {
  self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim())
})

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url)
  if (event.request.method === 'POST' && url.pathname === '/share-target') {
    event.respondWith(handleShareTarget(event))
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

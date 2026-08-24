import { createRouter, createWebHistory } from 'vue-router'

import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  // Fase 11 Task 3: "il ripristino della posizione di scorrimento non è
  // implementato: tornando in una sezione si riparte dall'alto" —
  // dichiarato esplicitamente come assente nel prototipo (documento
  // funzionale §7.6), non un comportamento da riprodurre. `savedPosition`
  // è valorizzato da vue-router solo per una navigazione avanti/indietro
  // nella cronologia — esattamente il caso "tornando in una vista" del
  // piano — mai per un click che apre una nuova vista, che deve sempre
  // ripartire dall'alto.
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition ?? { top: 0 }
  },
  routes: [
    { path: '/', component: () => import('@/views/TimelineView.vue'), meta: { auth: true } },
    { path: '/favorites', component: () => import('@/views/FavoritesView.vue'), meta: { auth: true } },
    { path: '/folders', component: () => import('@/views/FoldersView.vue'), meta: { auth: true } },
    { path: '/search', component: () => import('@/views/SearchView.vue'), meta: { auth: true } },
    { path: '/problems', component: () => import('@/views/ProblemsView.vue'), meta: { auth: true } },
    { path: '/duplicates', component: () => import('@/views/DuplicatesView.vue'), meta: { auth: true } },
    // Chunk lazy come mappa e impostazioni (§10.9): il budget dei 150 KB
    // iniziali riguarda solo ciò che `index.html` carica subito.
    { path: '/map', component: () => import('@/views/MapView.vue'), meta: { auth: true } },
    {
      path: '/settings',
      component: () => import('@/views/settings/SettingsView.vue'),
      meta: { auth: true }
    },
    {
      path: '/profile',
      component: () => import('@/views/ProfileView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/maps/offline',
      component: () => import('@/views/settings/MapsOfflineView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/webdav',
      component: () => import('@/views/settings/WebdavSetupView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/security/totp',
      component: () => import('@/views/settings/TotpSetupView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/backup',
      component: () => import('@/views/settings/BackupView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/restore',
      component: () => import('@/views/settings/RestoreView.vue'),
      meta: { auth: true }
    },
    {
      path: '/settings/sync',
      component: () => import('@/views/settings/SyncProbeView.vue'),
      meta: { auth: true }
    },
    {
      path: '/player/:id',
      component: () => import('@/views/PlayerView.vue'),
      meta: { auth: true }
    },
    { path: '/culling', component: () => import('@/views/CullingView.vue'), meta: { auth: true } },
    // PWA Share Target (§4.2 fase-5): il service worker redirige qui dopo
    // aver intercettato un POST "Condividi -> Keeppix" dalla galleria.
    {
      path: '/share-target',
      component: () => import('@/views/ShareTargetView.vue'),
      meta: { auth: true }
    },
    { path: '/tags', component: () => import('@/views/TagsView.vue'), meta: { auth: true } },
    { path: '/review', component: () => import('@/views/ReviewView.vue'), meta: { auth: true } },
    { path: '/albums', component: () => import('@/views/AlbumsView.vue'), meta: { auth: true } },
    // Segmento statico prima del parametrico: vue-router 4 preferisce già
    // `/albums/new` a `/albums/:id` a parità di specificità, ma l'ordine
    // qui lo rende anche leggibile a chi legge la lista.
    {
      path: '/albums/new',
      component: () => import('@/views/AlbumCreateView.vue'),
      meta: { auth: true }
    },
    {
      path: '/albums/:id',
      component: () => import('@/views/AlbumDetailView.vue'),
      meta: { auth: true }
    },
    // Solo shell mobile (§6): la quarta scheda "Altro" della tab bar
    // (Task 6, prossimo sotto-passo). Nessun link diretto da nessuna
    // parte della shell desktop.
    { path: '/more', component: () => import('@/views/MoreView.vue'), meta: { auth: true } },
    { path: '/shares', component: () => import('@/views/SharesView.vue'), meta: { auth: true } },
    { path: '/trash', component: () => import('@/views/TrashView.vue'), meta: { auth: true } },
    { path: '/users', component: () => import('@/views/UsersView.vue'), meta: { auth: true } },
    { path: '/groups', component: () => import('@/views/GroupsView.vue'), meta: { auth: true } },
    { path: '/batch-edit', component: () => import('@/views/BatchEditView.vue'), meta: { auth: true } },
    { path: '/s/:token', component: () => import('@/views/public/SharedView.vue') },
    { path: '/login', component: () => import('@/views/LoginView.vue') },
    { path: '/setup', component: () => import('@/views/SetupView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' }
  ]
})

router.beforeEach(async (to) => {
  const session = useSessionStore()
  if (!session.ready) {
    await session.bootstrap()
  }

  if (session.unavailable) {
    return true
  }

  // Istanza vergine: qualsiasi percorso porta al setup.
  if (session.initialised === false) {
    return to.path === '/setup' ? true : '/setup'
  }
  if (to.path === '/setup') {
    return '/'
  }
  if (to.meta.auth && !session.user) {
    return '/login'
  }
  if (to.path === '/login' && session.user) {
    return '/'
  }
  return true
})

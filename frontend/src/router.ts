import { createRouter, createWebHistory } from 'vue-router'

import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  // Scroll position restoration was explicitly absent from the prototype,
  // not a behavior to reproduce here. `savedPosition` is only set by
  // vue-router for a back/forward history navigation — exactly the
  // "returning to a view" case — never for a click that opens a new view,
  // which should always start at the top.
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
    // Lazy chunk, same as map and settings: the initial 150 KB budget only
    // covers what `index.html` loads right away.
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
    { path: '/culling', component: () => import('@/views/CullingLotsView.vue'), meta: { auth: true } },
    {
      path: '/culling/:lotId',
      component: () => import('@/views/CullingLotView.vue'),
      meta: { auth: true }
    },
    // PWA Share Target: the service worker redirects here after
    // intercepting a "Share -> Keeppix" POST from the gallery.
    {
      path: '/share-target',
      component: () => import('@/views/ShareTargetView.vue'),
      meta: { auth: true }
    },
    { path: '/tags', component: () => import('@/views/TagsView.vue'), meta: { auth: true } },
    { path: '/review', component: () => import('@/views/ReviewView.vue'), meta: { auth: true } },
    { path: '/persons', component: () => import('@/views/PeopleView.vue'), meta: { auth: true } },
    {
      path: '/persons/:id',
      component: () => import('@/views/PersonDetailView.vue'),
      meta: { auth: true }
    },
    { path: '/albums', component: () => import('@/views/AlbumsView.vue'), meta: { auth: true } },
    // Static segment before the parametric one: vue-router 4 already
    // prefers `/albums/new` over `/albums/:id` at equal specificity, but
    // the order here also makes it readable at a glance.
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
    // Mobile shell only: the fourth "More" tab in the tab bar. No direct
    // link from anywhere in the desktop shell.
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

  // Fresh instance: any path leads to setup.
  if (session.initialised === false) {
    return to.path === '/setup' ? true : '/setup'
  }
  // Admin account exists but the wizard never got to add a library — its
  // step lives only in SetupView's local state, so a reload mid-wizard
  // (e.g. while fixing a permission issue on the library path) otherwise
  // strands the session with no way back in, since /setup is normally
  // unreachable once initialised. SetupView itself resumes at the
  // library step, not admin creation, when it sees an existing user.
  if (session.user && session.hasLibrary === false) {
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

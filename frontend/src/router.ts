import { createRouter, createWebHistory } from 'vue-router'

import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: () => import('@/views/TimelineView.vue'), meta: { auth: true } },
    { path: '/folders', component: () => import('@/views/FoldersView.vue'), meta: { auth: true } },
    { path: '/search', component: () => import('@/views/SearchView.vue'), meta: { auth: true } },
    { path: '/problems', component: () => import('@/views/ProblemsView.vue'), meta: { auth: true } },
    // Chunk lazy come mappa e impostazioni (§10.9): il budget dei 150 KB
    // iniziali riguarda solo ciò che `index.html` carica subito.
    { path: '/culling', component: () => import('@/views/CullingView.vue'), meta: { auth: true } },
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

import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import type { User } from '@/api/auth'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import { useSessionStore } from '@/stores/session'

import MoreView from './MoreView.vue'

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

const testUser: User = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin',
  locale: null
}

async function mountMore(user: User = testUser) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/more', component: { template: '<div />' } },
      { path: '/folders', component: { template: '<div />' } },
      { path: '/map', component: { template: '<div />' } },
      { path: '/shares', component: { template: '<div />' } },
      { path: '/trash', component: { template: '<div />' } },
      { path: '/problems', component: { template: '<div />' } },
      { path: '/users', component: { template: '<div />' } },
      { path: '/groups', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = user

  await router.push('/more')
  await router.isReady()
  const wrapper = mount(MoreView, { global: { plugins: [router, i18n] } })
  mounted = wrapper
  return wrapper
}

describe('MoreView', () => {
  it('lists every real destination not on the tab bar, as real anchors', async () => {
    const wrapper = await mountMore()
    const hrefs = wrapper.findAll('a').map((a) => a.attributes('href'))
    expect(hrefs).toEqual(
      expect.arrayContaining(['/folders', '/map', '/shares', '/favorites', '/trash', '/problems', '/users', '/groups'])
    )
    for (const a of wrapper.findAll('a')) {
      expect(a.element.tagName).toBe('A')
    }
  })

  it('"Condivisioni" is a single row — SharesView has no separate withme/mine tabs to split it into two', async () => {
    const wrapper = await mountMore()
    const shareLinks = wrapper.findAll('a').filter((a) => a.attributes('href') === '/shares')
    expect(shareLinks).toHaveLength(1)
  })

  it('hides "Amministrazione" (Utenti/Gruppi) entirely for a non-admin user', async () => {
    const wrapper = await mountMore({ ...testUser, role: 'user' })
    const hrefs = wrapper.findAll('a').map((a) => a.attributes('href'))
    expect(hrefs).not.toContain('/users')
    expect(hrefs).not.toContain('/groups')
  })

  describe('background activity — AiAnalysis/FaceDetection', () => {
    it('shows nothing when no background operation is running', async () => {
      const wrapper = await mountMore()
      expect(wrapper.text()).not.toContain('Attività in background')
    })

    it('shows AI analysis progress from a live "embedding" phase event', async () => {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const wrapper = await mountMore()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 200, total: 3000, phase: 'embedding' }
      })
      await wrapper.vm.$nextTick()

      expect(wrapper.text()).toContain('Attività in background')
      expect(wrapper.text()).toContain('Analisi IA in corso — 200 di 3000')
    })

    it('shows face detection progress from a live "detecting" phase event', async () => {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const wrapper = await mountMore()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-2', done: 40, total: null, phase: 'detecting' }
      })
      await wrapper.vm.$nextTick()

      expect(wrapper.text()).toContain('Riconoscimento volti in corso — 40 finora')
    })

    it('ignores library-scan and bulk-rename phases — those have their own UI elsewhere', async () => {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const wrapper = await mountMore()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-3', done: 1, total: null, phase: '' }
      })
      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-4', done: 1, total: null, phase: 'renaming' }
      })
      await wrapper.vm.$nextTick()

      expect(wrapper.text()).not.toContain('Attività in background')
    })

    it('removes the card once the operation reaches a terminal phase', async () => {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const wrapper = await mountMore()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 200, total: 3000, phase: 'embedding' }
      })
      await wrapper.vm.$nextTick()
      expect(wrapper.text()).toContain('Attività in background')

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 3000, total: 3000, phase: 'done' }
      })
      await wrapper.vm.$nextTick()

      expect(wrapper.text()).not.toContain('Attività in background')
    })
  })
})

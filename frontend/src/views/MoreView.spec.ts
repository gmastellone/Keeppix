import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import type { User } from '@/api/auth'
import { useSessionStore } from '@/stores/session'

import MoreView from './MoreView.vue'

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
      expect.arrayContaining(['/folders', '/map', '/shares', '/trash', '/problems', '/users', '/groups'])
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
})

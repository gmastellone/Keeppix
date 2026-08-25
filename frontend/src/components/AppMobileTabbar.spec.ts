import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

import AppMobileTabbar from './AppMobileTabbar.vue'

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

async function mountTabbar(path: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/search', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } },
      { path: '/more', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } },
      { path: '/batch-edit', component: { template: '<div />' } },
      { path: '/trash', component: { template: '<div />' } }
    ]
  })
  await router.push(path)
  await router.isReady()
  const wrapper = mount(AppMobileTabbar, { global: { plugins: [router, i18n] } })
  mounted = wrapper
  return wrapper
}

describe('AppMobileTabbar', () => {
  it('has exactly the four documented tabs, in order, as real anchors', async () => {
    const wrapper = await mountTabbar('/')
    const links = wrapper.findAll('a')
    expect(links.map((a) => a.attributes('href'))).toEqual(['/', '/search', '/albums', '/more'])
    expect(links.map((a) => a.text())).toEqual(['Foto', 'Cerca', 'Album', 'Altro'])
  })

  it.each([
    ['/', 'Foto'],
    ['/search', 'Cerca'],
    ['/albums', 'Album'],
    ['/more', 'Altro']
  ])('highlights only the "%s" tab when the route is %s', async (path, expectedLabel) => {
    const wrapper = await mountTabbar(path)
    const active = wrapper.findAll('a').filter((a) => a.classes().includes('text-accent'))
    expect(active).toHaveLength(1)
    expect(active[0].text()).toBe(expectedLabel)
  })

  it.each(['/culling', '/batch-edit'])(
    'highlights "Foto", not "Altro", on %s — entered only from the Foto view (documented rule)',
    async (path) => {
      const wrapper = await mountTabbar(path)
      const active = wrapper.findAll('a').filter((a) => a.classes().includes('text-accent'))
      expect(active).toHaveLength(1)
      expect(active[0].text()).toBe('Foto')
    }
  )

  it('highlights "Altro" for a route that only exists on the MoreView tree', async () => {
    const wrapper = await mountTabbar('/trash')
    const active = wrapper.findAll('a').filter((a) => a.classes().includes('text-accent'))
    expect(active).toHaveLength(1)
    expect(active[0].text()).toBe('Altro')
  })
})

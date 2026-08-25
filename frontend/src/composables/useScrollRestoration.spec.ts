import { mount } from '@vue/test-utils'
import { defineComponent, ref } from 'vue'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { describe, expect, it } from 'vitest'

import { useScrollRestoration } from './useScrollRestoration'

function makeRouter(path: string): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path, component: { template: '<div />' } }]
  })
}

function makeHost() {
  return defineComponent({
    setup() {
      const el = ref<HTMLElement | null>(null)
      useScrollRestoration(el)
      return { el }
    },
    template: '<div ref="el" style="height: 100px; overflow: auto"><div style="height: 2000px" /></div>'
  })
}

describe('useScrollRestoration', () => {
  it('restores the scroll position of a fresh DOM element mounted for the same route', async () => {
    const router = makeRouter('/scroll-a')
    await router.push('/scroll-a')
    await router.isReady()

    const Host = makeHost()
    const first = mount(Host, { global: { plugins: [router] } })
    first.vm.el!.scrollTop = 456
    first.unmount()
    // vue-router resetta `currentRoute` a START_LOCATION quando l'ultima app
    // che lo usa fa unmount (pulizia interna, vedi vue-router `installedApps`)
    // — un artefatto di questo test che monta/smonta un'intera app Vue in
    // sequenza, non qualcosa che accade in una SPA reale (dove l'app non
    // viene mai smontata, solo le viste al suo interno). Una navigazione
    // vera arriva già sulla rotta di destinazione prima che il nuovo
    // componente venga montato, quindi si ripristina qui lo stesso ordine.
    await router.push('/scroll-a')

    const second = mount(Host, { global: { plugins: [router] } })
    await second.vm.$nextTick()

    expect(second.vm.el!.scrollTop).toBe(456)
  })

  it('a fresh route with no saved position starts at the top', async () => {
    const router = makeRouter('/scroll-b')
    await router.push('/scroll-b')
    await router.isReady()

    const Host = makeHost()
    const wrapper = mount(Host, { global: { plugins: [router] } })
    await wrapper.vm.$nextTick()

    expect(wrapper.vm.el!.scrollTop).toBe(0)
  })

  it('an explicit key keeps positions distinct even for the same route path', async () => {
    const router = makeRouter('/scroll-c')
    await router.push('/scroll-c')
    await router.isReady()

    const HostWithKey = defineComponent({
      props: { cacheKey: { type: String, required: true } },
      setup(props) {
        const el = ref<HTMLElement | null>(null)
        useScrollRestoration(el, props.cacheKey)
        return { el }
      },
      template: '<div ref="el" style="height: 100px; overflow: auto"><div style="height: 2000px" /></div>'
    })

    const first = mount(HostWithKey, { props: { cacheKey: 'batch-1' }, global: { plugins: [router] } })
    first.vm.el!.scrollTop = 200
    first.unmount()

    const second = mount(HostWithKey, { props: { cacheKey: 'batch-2' }, global: { plugins: [router] } })
    await second.vm.$nextTick()

    expect(second.vm.el!.scrollTop).toBe(0)
  })
})

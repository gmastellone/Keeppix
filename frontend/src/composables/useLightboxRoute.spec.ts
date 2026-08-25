import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { describe, expect, it, vi } from 'vitest'

import { useLightboxRoute } from './useLightboxRoute'

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

interface Asset {
  id: string
}

function makeRouter(): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }]
  })
}

function mountHost(
  router: Router,
  findLocal: (id: string) => Asset | undefined,
  loadRemote: (id: string) => Promise<Asset>
) {
  let lightbox!: ReturnType<typeof useLightboxRoute<Asset>>
  const Host = defineComponent({
    setup() {
      lightbox = useLightboxRoute<Asset>(findLocal, loadRemote)
      return {}
    },
    template: '<div />'
  })
  const wrapper = mount(Host, { global: { plugins: [router] } })
  return {
    wrapper,
    get lightbox() {
      return lightbox
    }
  }
}

describe('useLightboxRoute', () => {
  it('opening a known asset pushes ?photo= and sets viewing from the local list, without calling loadRemote', async () => {
    const router = makeRouter()
    await router.push('/')
    const asset = { id: 'a1' }
    const loadRemote = vi.fn()
    const { lightbox } = mountHost(router, (id) => (id === 'a1' ? asset : undefined), loadRemote)

    await lightbox.open(asset)

    expect(router.currentRoute.value.query.photo).toBe('a1')
    expect(lightbox.viewing.value).toEqual(asset)
    expect(loadRemote).not.toHaveBeenCalled()
  })

  it('a browser back after opening clears the query and viewing (the URL is the source of truth)', async () => {
    const router = makeRouter()
    await router.push('/')
    const asset = { id: 'a1' }
    const { lightbox, wrapper } = mountHost(router, () => asset, vi.fn())

    await lightbox.open(asset)
    router.back()
    await tick()
    await wrapper.vm.$nextTick()

    expect(router.currentRoute.value.query.photo).toBeUndefined()
    expect(lightbox.viewing.value).toBeNull()
  })

  it('stepping to the next photo replaces the query instead of pushing a new history entry', async () => {
    const router = makeRouter()
    await router.push('/')
    const a1 = { id: 'a1' }
    const a2 = { id: 'a2' }
    const { lightbox } = mountHost(router, (id) => [a1, a2].find((a) => a.id === id), vi.fn())

    await lightbox.open(a1)
    const pushSpy = vi.spyOn(router, 'push')
    const replaceSpy = vi.spyOn(router, 'replace')

    await lightbox.step(a2)

    expect(router.currentRoute.value.query.photo).toBe('a2')
    expect(lightbox.viewing.value).toEqual(a2)
    expect(replaceSpy).toHaveBeenCalledOnce()
    expect(pushSpy).not.toHaveBeenCalled()
  })

  it('closing after opening via push goes back instead of leaving a dangling forward entry', async () => {
    const router = makeRouter()
    await router.push('/')
    const asset = { id: 'a1' }
    const { lightbox, wrapper } = mountHost(router, () => asset, vi.fn())

    await lightbox.open(asset)
    const backSpy = vi.spyOn(router, 'back')

    lightbox.close()
    await tick()
    await wrapper.vm.$nextTick()

    expect(backSpy).toHaveBeenCalledOnce()
    expect(router.currentRoute.value.query.photo).toBeUndefined()
  })

  it('closing without ever having opened (a direct link) strips the query without calling back()', async () => {
    const router = makeRouter()
    await router.push({ path: '/', query: { photo: 'a1' } })
    const asset = { id: 'a1' }
    const { lightbox, wrapper } = mountHost(router, () => asset, vi.fn())
    await wrapper.vm.$nextTick()
    expect(lightbox.viewing.value).toEqual(asset)
    const backSpy = vi.spyOn(router, 'back')

    await lightbox.close()

    expect(backSpy).not.toHaveBeenCalled()
    expect(router.currentRoute.value.query.photo).toBeUndefined()
    // Non è mai uscita dall'app: siamo ancora sulla stessa rotta.
    expect(router.currentRoute.value.path).toBe('/')
  })

  it('reloading directly on a ?photo= URL restores viewing via loadRemote when the asset is not locally known', async () => {
    const router = makeRouter()
    await router.push({ path: '/', query: { photo: 'a1' } })
    const remoteAsset = { id: 'a1' }
    const loadRemote = vi.fn().mockResolvedValue(remoteAsset)
    const { lightbox, wrapper } = mountHost(router, () => undefined, loadRemote)
    await wrapper.vm.$nextTick()
    await wrapper.vm.$nextTick()

    expect(loadRemote).toHaveBeenCalledWith('a1')
    expect(lightbox.viewing.value).toEqual(remoteAsset)
  })

  it('openById replaces the query (used for the AssetViewer "open-asset" cross-link, not a fresh push)', async () => {
    const router = makeRouter()
    await router.push('/')
    const a1 = { id: 'a1' }
    const a2 = { id: 'a2' }
    const { lightbox } = mountHost(router, (id) => [a1, a2].find((a) => a.id === id), vi.fn())

    await lightbox.open(a1)
    const pushSpy = vi.spyOn(router, 'push')
    const replaceSpy = vi.spyOn(router, 'replace')

    await lightbox.openById('a2')

    expect(router.currentRoute.value.query.photo).toBe('a2')
    expect(replaceSpy).toHaveBeenCalledOnce()
    expect(pushSpy).not.toHaveBeenCalled()
  })
})

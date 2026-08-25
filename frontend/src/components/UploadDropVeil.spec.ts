import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'
import { useUploadStore } from '@/stores/upload'

import UploadDropVeil from './UploadDropVeil.vue'

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  setActivePinia(createPinia())
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

function dragEvent(type: string, opts: { hasFiles?: boolean; files?: File[] } = {}): Event {
  const event = new Event(type, { bubbles: true, cancelable: true })
  Object.defineProperty(event, 'dataTransfer', {
    value: {
      types: opts.hasFiles === false ? ['text/plain'] : ['Files'],
      files: opts.files ?? [],
      dropEffect: 'none'
    },
    writable: true
  })
  return event
}

function file(name: string): File {
  return new File([new Blob(['x'])], name)
}

async function mountVeil(path = '/') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } }
    ]
  })
  await router.push(path)
  await router.isReady()
  const upload = useUploadStore()
  const toast = useToastStore()
  const wrapper = mount(UploadDropVeil, { global: { plugins: [router, i18n] } })
  mounted = wrapper
  await flushPromises()
  return { wrapper, upload, toast }
}

describe('UploadDropVeil', () => {
  it('is invisible at rest — zero pixels until a real file drag starts (§7.4)', async () => {
    const { wrapper } = await mountVeil()
    expect(wrapper.find('div').exists()).toBe(false)
  })

  it('shows the veil on dragenter carrying files, with the exact documented text', async () => {
    const { wrapper } = await mountVeil()
    window.dispatchEvent(dragEvent('dragenter'))
    await flushPromises()

    expect(wrapper.text()).toContain('Rilascia le foto qui')
    expect(wrapper.text()).toContain('i RAW si caricano dal Culling')
  })

  it('ignores a drag that carries no files (e.g. dragging text/an image from another tab)', async () => {
    const { wrapper } = await mountVeil()
    window.dispatchEvent(dragEvent('dragenter', { hasFiles: false }))
    await flushPromises()

    expect(wrapper.find('div').exists()).toBe(false)
  })

  it('dragenter and dragover both preventDefault — required for the browser not to open the file itself', async () => {
    await mountVeil()
    const enter = dragEvent('dragenter')
    window.dispatchEvent(enter)
    const over = dragEvent('dragover')
    window.dispatchEvent(over)

    expect(enter.defaultPrevented).toBe(true)
    expect(over.defaultPrevented).toBe(true)
  })

  it('tracks nesting depth — leaving a child element does not hide the veil, only the last dragleave does', async () => {
    const { wrapper } = await mountVeil()
    window.dispatchEvent(dragEvent('dragenter'))
    window.dispatchEvent(dragEvent('dragenter')) // il puntatore passa su un figlio
    await flushPromises()
    expect(wrapper.text()).toContain('Rilascia le foto qui')

    window.dispatchEvent(dragEvent('dragleave'))
    await flushPromises()
    expect(wrapper.text()).toContain('Rilascia le foto qui') // ancora dentro, un solo dragleave

    window.dispatchEvent(dragEvent('dragleave'))
    await flushPromises()
    expect(wrapper.find('div').exists()).toBe(false)
  })

  it('drop outside Culling classifies and queues the files, then hides the veil', async () => {
    const { wrapper, upload } = await mountVeil('/')
    const spy = vi.spyOn(upload, 'addFilesFromPicker').mockImplementation(async () => {})
    window.dispatchEvent(dragEvent('dragenter'))
    await flushPromises()

    const dropped = [file('a.jpg')]
    window.dispatchEvent(dragEvent('drop', { files: dropped }))
    await flushPromises()

    expect(spy).toHaveBeenCalledWith(dropped)
    expect(wrapper.find('div').exists()).toBe(false)
  })

  it('a drop on /culling is rejected with the exact documented toast, and never reaches the upload store', async () => {
    const { upload, toast } = await mountVeil('/culling')
    const spy = vi.spyOn(upload, 'addFilesFromPicker').mockImplementation(async () => {})
    window.dispatchEvent(dragEvent('dragenter'))
    await flushPromises()

    window.dispatchEvent(dragEvent('drop', { files: [file('a.jpg')] }))
    await flushPromises()

    expect(spy).not.toHaveBeenCalled()
    expect(toast.toasts.at(-1)?.message).toBe(
      'Il Culling ha un suo percorso di importazione: qui non si rilasciano file.'
    )
    expect(toast.toasts.at(-1)?.kind).toBe('error')
  })
})

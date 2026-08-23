import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'
import { useUploadStore, type UploadSessionState } from '@/stores/upload'

import UploadQueueStrip from './UploadQueueStrip.vue'

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

function session(overrides: Partial<UploadSessionState>): UploadSessionState {
  return {
    id: 'a',
    filename: 'a.jpg',
    targetFolderId: 'folder-1',
    expectedSize: 10,
    receivedBytes: 0,
    status: 'queued',
    ...overrides
  }
}

function mountStrip() {
  const upload = useUploadStore()
  const wrapper = mount(UploadQueueStrip, { global: { plugins: [i18n] } })
  mounted = wrapper
  return { wrapper, upload }
}

describe('UploadQueueStrip', () => {
  it('does not exist at all with an empty queue — zero pixels at rest (§6.1)', () => {
    const { wrapper } = mountStrip()
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('does not exist for an all-rejected batch either — the dock only ever looks at sessions, verified against renderUploadDock() (mockup riga 2913)', () => {
    const { wrapper, upload } = mountStrip()
    upload.rejectedRaw = ['a.arw']
    return wrapper.vm.$nextTick().then(() => {
      expect(wrapper.find('button').exists()).toBe(false)
    })
  })

  it('shows "Scegli dove" — and the accent ring — when a session is stuck without a destination', () => {
    const { wrapper, upload } = mountStrip()
    upload.sessions.push(session({ targetFolderId: null }))
    return wrapper.vm.$nextTick().then(() => {
      expect(wrapper.text()).toContain('Scegli dove')
      expect(wrapper.find('button').attributes('class')).toContain('shadow-')
    })
  })

  it('shows "In pausa" when every pending session is paused', () => {
    const { wrapper, upload } = mountStrip()
    upload.sessions.push(session({ status: 'paused' }))
    return wrapper.vm.$nextTick().then(() => {
      expect(wrapper.text()).toContain('In pausa')
    })
  })

  it('shows "Caricamento" while at least one session is actively uploading', () => {
    const { wrapper, upload } = mountStrip()
    upload.sessions.push(session({ status: 'uploading' }), session({ id: 'b', status: 'paused' }))
    return wrapper.vm.$nextTick().then(() => {
      expect(wrapper.text()).toContain('Caricamento')
    })
  })

  it('shows "Caricate" and a full bar once nothing is pending anymore', () => {
    const { wrapper, upload } = mountStrip()
    upload.sessions.push(session({ status: 'done', receivedBytes: 10 }))
    return wrapper.vm.$nextTick().then(() => {
      expect(wrapper.text()).toContain('Caricate')
      expect(wrapper.text()).toContain('1/1')
    })
  })

  it('clicking the strip toggles the shared panelOpen state', async () => {
    const { wrapper, upload } = mountStrip()
    upload.sessions.push(session({}))
    await wrapper.vm.$nextTick()

    expect(upload.panelOpen).toBe(false)
    await wrapper.find('button').trigger('click')
    expect(upload.panelOpen).toBe(true)
  })
})

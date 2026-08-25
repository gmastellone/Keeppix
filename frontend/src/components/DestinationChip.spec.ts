import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { useUploadStore } from '@/stores/upload'

import DestinationChip from './DestinationChip.vue'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn(async () => ({
    user: { id: '1', username: 'admin', display_name: 'Admin', email: null, role: 'admin', locale: null },
    folders: [
      { id: 'f1', library_id: 'l1', parent_id: null, name: 'Urbino', depth: 0 },
      { id: 'f2', library_id: 'l1', parent_id: null, name: 'Lago di Braies', depth: 0 }
    ],
    storage: {},
    badges: { culling: 0, revision: 0 }
  }))
}))

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  vi.resetAllMocks()
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

async function mountChip() {
  setActivePinia(createPinia())
  const upload = useUploadStore()
  const wrapper = mount(DestinationChip, { global: { plugins: [i18n] }, attachTo: document.body })
  mounted = wrapper
  await flushPromises()
  return { wrapper, upload }
}

describe('DestinationChip', () => {
  it('shows the tenue-orange "missing" state when no destination is resolved yet', async () => {
    const { wrapper } = await mountChip()
    expect(wrapper.text()).toContain('Scegli una cartella')
    expect(wrapper.text()).toContain('Le foto restano in coda')
    expect(wrapper.find('button').classes()).toContain('bg-accent-tint')
  })

  it('shows the real folder name once a destination is resolved', async () => {
    const { wrapper, upload } = await mountChip()
    upload.sessions.push({
      id: 'a',
      filename: 'a.jpg',
      targetFolderId: 'f1',
      expectedSize: 10,
      receivedBytes: 0,
      status: 'queued'
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Urbino')
    expect(wrapper.text()).not.toContain('Scegli una cartella')
  })

  it('lists every real folder in the listbox, and picking one calls setDestination', async () => {
    const { wrapper, upload } = await mountChip()
    await wrapper.find('button').trigger('click')
    await tick()

    const listbox = document.body.querySelector('[role="listbox"]')
    expect(listbox).toBeDefined()
    const options = Array.from(document.body.querySelectorAll('[role="option"]'))
    expect(options.map((o) => o.textContent?.trim())).toEqual(['Urbino', 'Lago di Braies'])

    const urbino = options.find((o) => o.textContent?.trim() === 'Urbino') as HTMLElement
    urbino.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(upload.stickyDestination).toBeNull() // nessuna sessione in coda: setDestination non ha nulla da assegnare
  })

  it('setDestination actually unblocks a queued destinationless session', async () => {
    const { wrapper, upload } = await mountChip()
    upload.sessions.push({
      id: 'a',
      filename: 'a.jpg',
      targetFolderId: null,
      expectedSize: 10,
      receivedBytes: 0,
      status: 'queued'
    })

    await wrapper.find('button').trigger('click')
    await tick()
    const urbino = Array.from(document.body.querySelectorAll('[role="option"]')).find(
      (o) => o.textContent?.trim() === 'Urbino'
    ) as HTMLElement
    urbino.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(upload.sessions[0].targetFolderId).toBe('f1')
  })
})

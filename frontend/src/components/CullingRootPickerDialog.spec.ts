import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { FolderView } from '@/api/folders'
import { i18n } from '@/i18n'

import CullingRootPickerDialog from './CullingRootPickerDialog.vue'

const fetchChildrenMock = vi.fn()

vi.mock('@/api/folders', () => ({
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

function folder(overrides: Partial<FolderView> = {}): FolderView {
  return {
    id: 'root',
    library_id: 'lib-1',
    parent_id: null,
    name: 'Lago di Braies',
    depth: 0,
    ...overrides
  }
}

const root = folder()
const culling = folder({ id: 'culling', parent_id: 'root', name: 'Culling', depth: 1 })
const archivio = folder({ id: 'archivio', parent_id: 'culling', name: 'Archivio', depth: 2 })

let wrapper: VueWrapper | undefined

// `open` is a required v-model prop: it needs a host component with its
// own state (`ConfirmDialog.spec.ts` uses the same pattern), not a direct
// mount with a static prop — `defineModel` only stays in sync if something
// actually writes `open` back in response to the emitted event.
function mountHost(initialPath: FolderView[] = [root]) {
  const Host = defineComponent({
    components: { ThePicker: CullingRootPickerDialog },
    emits: ['confirm'],
    setup(_, { emit }) {
      const open = ref(false)
      return { open, initialPath, onConfirm: (id: string) => emit('confirm', id) }
    },
    template: `
      <button ref="trigger" type="button" @click="open = true">Cambia…</button>
      <ThePicker v-model:open="open" :initial-path="initialPath" @confirm="onConfirm" />
    `
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

async function openViaTrigger(w: VueWrapper) {
  const trigger = w.get('button')
  trigger.element.focus()
  await trigger.trigger('click')
  await flushPromises()
  return trigger
}

describe('CullingRootPickerDialog', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'it'
    fetchChildrenMock.mockResolvedValue({ folders: [culling], assets: [] })
  })

  // reka-ui's `DialogPortal` always teleports into the real `document.body`:
  // without explicitly unmounting, one test's markup stays there for the next.
  afterEach(() => {
    wrapper?.unmount()
    wrapper = undefined
  })

  it('opens positioned at the initial path, root crumb first, and loads its children', async () => {
    const w = mountHost([root])
    await openViaTrigger(w)

    expect(fetchChildrenMock).toHaveBeenCalledWith('root')
    const dialog = document.body.querySelector('[role="dialog"]')!
    expect(dialog.textContent).toContain('/')
    expect(dialog.textContent).toContain('Culling')
  })

  it('clicking a folder row descends and replaces the list with its children', async () => {
    const w = mountHost([root])
    await openViaTrigger(w)
    fetchChildrenMock.mockResolvedValueOnce({ folders: [archivio], assets: [] })

    const row = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.includes('Culling'))
    row?.click()
    await flushPromises()

    expect(fetchChildrenMock).toHaveBeenCalledWith('culling')
    const dialog = document.body.querySelector('[role="dialog"]')!
    expect(dialog.textContent).toContain('Archivio')
    expect(dialog.textContent).not.toContain('Culling / Culling')
  })

  it('clicking the root breadcrumb from a deeper level truncates the path', async () => {
    const w = mountHost([root, culling])
    fetchChildrenMock.mockResolvedValueOnce({ folders: [archivio], assets: [] })
    await openViaTrigger(w)

    fetchChildrenMock.mockResolvedValueOnce({ folders: [culling], assets: [] })
    const rootCrumb = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === '/')
    rootCrumb?.click()
    await flushPromises()

    expect(fetchChildrenMock).toHaveBeenLastCalledWith('root')
  })

  it('shows the empty-state copy when a level has no subfolders', async () => {
    fetchChildrenMock.mockResolvedValueOnce({ folders: [], assets: [] })
    const w = mountHost([root])
    await openViaTrigger(w)

    expect(document.body.querySelector('[role="dialog"]')!.textContent).toContain('Nessuna sottocartella qui.')
  })

  it('"Usa questa cartella" emits confirm with the current folder and closes', async () => {
    const w = mountHost([root, culling])
    fetchChildrenMock.mockResolvedValueOnce({ folders: [archivio], assets: [] })
    await openViaTrigger(w)

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Usa questa cartella')
    confirmBtn?.click()
    await flushPromises()

    expect(w.emitted('confirm')).toEqual([['culling']])
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('"Annulla" closes without emitting', async () => {
    const w = mountHost([root])
    await openViaTrigger(w)

    const cancelBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Annulla')
    cancelBtn?.click()
    await flushPromises()

    expect(w.emitted('confirm')).toBeUndefined()
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('deviates from the shared Dialog.vue pattern: clicking the scrim does not close the dialog', async () => {
    const w = mountHost([root])
    await openViaTrigger(w)

    const scrim = document.body.querySelector('.fixed.inset-0.z-40') as HTMLElement
    scrim.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
    scrim.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()
  })

  it('Escape still closes the dialog', async () => {
    const w = mountHost([root])
    await openViaTrigger(w)

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()

    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('nothing receives focus automatically on open (no auto-focus, deviates from the shared Dialog.vue pattern)', async () => {
    const w = mountHost([root])
    const trigger = await openViaTrigger(w)

    const dialog = document.body.querySelector('[role="dialog"]')
    expect(dialog?.contains(document.activeElement)).toBe(false)
    expect(document.activeElement).toBe(trigger.element)
  })

  it('focus returns to the trigger that opened it, on close', async () => {
    const w = mountHost([root])
    const trigger = await openViaTrigger(w)

    const cancelBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Annulla')
    cancelBtn?.click()
    await flushPromises()

    expect(document.activeElement).toBe(trigger.element)
  })
})

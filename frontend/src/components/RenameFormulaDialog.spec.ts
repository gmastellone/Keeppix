import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { LiveMessage } from '@/api/events'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

const previewRenameMock = vi.fn()
const applyRenameBatchMock = vi.fn()
const cancelOperationMock = vi.fn()
const startLiveEventsMock = vi.fn<(cb: (msg: LiveMessage) => void) => { close: () => void }>(
  () => ({ close: vi.fn() })
)

vi.mock('@/api/rename', () => ({
  previewRename: (...args: unknown[]) => previewRenameMock(...args),
  applyRenameBatch: (...args: unknown[]) => applyRenameBatchMock(...args)
}))

vi.mock('@/api/operations', () => ({
  cancelOperation: (...args: unknown[]) => cancelOperationMock(...args)
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: (cb: (msg: LiveMessage) => void) => startLiveEventsMock(cb)
}))

const RenameFormulaDialog = (await import('./RenameFormulaDialog.vue')).default

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: null,
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: null,
    width: 100,
    height: 100,
    thumbhash: null,
    raw_kind: null,
    favorite: false,
    camera_model: null,
    tags: [],
    faces: []
  }
}

let wrapper: VueWrapper | undefined

function mountHost(assets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheRenameFormulaDialog: RenameFormulaDialog },
    setup() {
      const open = ref(true)
      return { open, assets }
    },
    template: `<TheRenameFormulaDialog v-model:open="open" :assets="assets" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

function mountHostWithSubfolders(assets: TimelineAsset[], restrictedAssets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheRenameFormulaDialog: RenameFormulaDialog },
    setup() {
      const open = ref(true)
      return { open, assets, restrictedAssets }
    },
    template: `<TheRenameFormulaDialog v-model:open="open" :assets="assets" :restricted-assets="restrictedAssets" has-subfolders />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

function schemaInput(): HTMLInputElement {
  const el = document.body.querySelector('input[type="text"]')
  if (!el) throw new Error('schema input not found')
  return el as HTMLInputElement
}

function schemaInputOrNull(): HTMLInputElement | null {
  return document.body.querySelector('input[type="text"]')
}

interface OperationProgressPayload {
  operation_id: string
  done: number
  total: number | null
  phase: string
}

/** The last `onEvent` registered with `startLiveEvents` — the dialog opens
 * one per mounted component, always the last one of interest in these
 * tests. */
function emitOperationProgress(payload: OperationProgressPayload) {
  const onEvent = startLiveEventsMock.mock.calls.at(-1)?.[0] as ((msg: LiveMessage) => void) | undefined
  onEvent?.({ v: 1, type: 'operation.progress', payload })
}

function buttonWithText(text: string): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  vi.useFakeTimers()
  i18n.global.locale.value = 'it'
  previewRenameMock.mockResolvedValue([])
  applyRenameBatchMock.mockResolvedValue({ operation_id: 'op1' })
  cancelOperationMock.mockResolvedValue({ succeeded: [], failed: [], batch_id: null })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.useRealTimers()
})

describe('RenameFormulaDialog', () => {
  it('opens with the default schema {data}_{luogo}_{n:3} and previews it immediately', async () => {
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    expect(schemaInput().value).toBe('{data}_{luogo}_{n:3}')
    expect(previewRenameMock).toHaveBeenCalledWith(['a'], '{data}_{luogo}_{n:3}')
  })

  it('shows up to 5 preview rows (current name → new name), all computed but only the first 5 shown', async () => {
    previewRenameMock.mockResolvedValue(
      Array.from({ length: 8 }, (_, i) => ({
        asset_id: `a${i}`,
        folder_id: 'f',
        current_name: `old${i}.jpg`,
        new_name: `new${i}.jpg`,
        collides: false
      }))
    )
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    const rows = document.body.querySelectorAll('li')
    expect(rows.length).toBe(5)
    expect(document.body.textContent).toContain('old0.jpg')
    expect(document.body.textContent).not.toContain('old5.jpg')
  })

  it('shows "Nessuna foto in questo ambito." when the scope preview is empty', async () => {
    mountHost([])
    await vi.runAllTimersAsync()

    expect(document.body.textContent).toContain('Nessuna foto in questo ambito.')
  })

  it('clicking a placeholder button inserts it at the cursor position and refocuses the field', async () => {
    mountHost([photo('a')])
    await vi.runAllTimersAsync()
    previewRenameMock.mockClear()

    // Cursor at the start of the field (default value untouched, never
    // rewritten by hand: changing it via the DOM wouldn't go through
    // v-model) — the placeholder button must insert there, not at the end.
    const input = schemaInput()
    input.setSelectionRange(0, 0)
    const titleBtn = buttonWithText('Titolo')
    titleBtn?.click()
    await vi.runAllTimersAsync()

    expect(schemaInput().value).toBe('{titolo}{data}_{luogo}_{n:3}')
    expect(document.activeElement).toBe(schemaInput())
    expect(previewRenameMock).toHaveBeenCalledWith(['a'], '{titolo}{data}_{luogo}_{n:3}')
  })

  it('a collision disables "Applica" and shows the warning; no collision leaves it enabled', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: 'same.jpg', collides: true },
      { asset_id: 'b', folder_id: 'f', current_name: 'b.jpg', new_name: 'same.jpg', collides: true }
    ])
    mountHost([photo('a'), photo('b')])
    await vi.runAllTimersAsync()

    const applyBtn = buttonWithText('Applica') as HTMLButtonElement
    expect(applyBtn.disabled).toBe(true)
    expect(document.body.textContent).toContain('2 nomi risulterebbero uguali tra loro')
  })

  it('"Applica" starts the batch and switches to a real progress view instead of closing', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    expect(applyRenameBatchMock).toHaveBeenCalledWith(['a'], '{data}_{luogo}_{n:3}')
    // A 202 comes back immediately, the work runs in the background — the
    // dialog stays open and shows real progress, it doesn't close until
    // the terminal WebSocket event arrives.
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()
    expect(document.body.textContent).toContain('Rinomina in corso')
    expect(schemaInputOrNull()).toBeNull()
  })

  it('updates the progress bar live from operation.progress WebSocket events', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    mountHost([photo('a')])
    await vi.runAllTimersAsync()
    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    emitOperationProgress({ operation_id: 'op1', done: 3, total: 10, phase: 'renaming' })
    await vi.runAllTimersAsync()

    expect(document.body.textContent).toContain('3 di 10')
  })

  it('a "done" event closes the dialog, shows the real count, and emits "applied"', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    const wrapper = mountHost([photo('a')])
    await vi.runAllTimersAsync()
    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    // `advanceTimersByTimeAsync(0)`, not `runAllTimersAsync`: the latter
    // would also exhaust the toast's auto-dismiss timer
    // (`stores/toast.ts::arm`), making it disappear before it can be read.
    emitOperationProgress({ operation_id: 'op1', done: 1, total: 1, phase: 'done' })
    await vi.advanceTimersByTimeAsync(0)

    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message.includes('1 foto rinominata'))).toBe(true)
    expect(wrapper.findComponent(RenameFormulaDialog).emitted('applied')).toBeTruthy()
  })

  it('a "failed" event closes the dialog with an error toast, ignoring events for a different operation', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    mountHost([photo('a')])
    await vi.runAllTimersAsync()
    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    // Someone else's operation must not touch this dialog.
    emitOperationProgress({ operation_id: 'op-someone-else', done: 5, total: 5, phase: 'done' })
    await vi.runAllTimersAsync()
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()

    emitOperationProgress({ operation_id: 'op1', done: 0, total: 1, phase: 'failed' })
    await vi.runAllTimersAsync()

    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('clicking "Annulla" during a rename calls cancelOperation and closes on success', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    cancelOperationMock.mockResolvedValue({ succeeded: ['a'], failed: [], batch_id: null })
    mountHost([photo('a')])
    await vi.runAllTimersAsync()
    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    buttonWithText('Annulla')?.click()
    // `advanceTimersByTimeAsync(0)`, not `runAllTimersAsync`: see the
    // comment in the "done" test above.
    await vi.advanceTimersByTimeAsync(0)

    expect(cancelOperationMock).toHaveBeenCalledWith('op1')
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message.includes('già rinominata'))).toBe(true)
  })

  it('"Annulla" closes without applying', async () => {
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    buttonWithText('Annulla')?.click()
    await vi.runAllTimersAsync()

    expect(applyRenameBatchMock).not.toHaveBeenCalled()
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('with hasSubfolders, the toggle is off by default and previews only the restricted scope', async () => {
    mountHostWithSubfolders([photo('a'), photo('b'), photo('c')], [photo('a')])
    await vi.runAllTimersAsync()

    expect(previewRenameMock).toHaveBeenCalledWith(['a'], '{data}_{luogo}_{n:3}')
  })

  it('switching the toggle on widens the preview and apply scope to the whole lot', async () => {
    mountHostWithSubfolders([photo('a'), photo('b'), photo('c')], [photo('a')])
    await vi.runAllTimersAsync()
    previewRenameMock.mockClear()
    previewRenameMock.mockResolvedValue(
      ['a', 'b', 'c'].map((id) => ({ asset_id: id, folder_id: 'f', current_name: `${id}.jpg`, new_name: `${id}2.jpg`, collides: false }))
    )

    const toggle = document.body.querySelector('[role="switch"]') as HTMLButtonElement
    toggle.click()
    await vi.runAllTimersAsync()

    expect(previewRenameMock).toHaveBeenCalledWith(['a', 'b', 'c'], '{data}_{luogo}_{n:3}')

    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    expect(applyRenameBatchMock).toHaveBeenCalledWith(['a', 'b', 'c'], '{data}_{luogo}_{n:3}')
  })

  it('the toggle resets to off on every fresh open, never remembered', async () => {
    mountHostWithSubfolders([photo('a'), photo('b')], [photo('a')])
    await vi.runAllTimersAsync()
    const toggle = document.body.querySelector('[role="switch"]') as HTMLButtonElement
    toggle.click()
    await vi.runAllTimersAsync()
    expect(toggle.getAttribute('aria-checked')).toBe('true')
    wrapper?.unmount()

    mountHostWithSubfolders([photo('a'), photo('b')], [photo('a')])
    await vi.runAllTimersAsync()

    expect(document.body.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('false')
  })

  it('without hasSubfolders, no toggle is rendered and the full assets scope is used', async () => {
    mountHost([photo('a'), photo('b')])
    await vi.runAllTimersAsync()

    expect(document.body.querySelector('[role="switch"]')).toBeNull()
    expect(previewRenameMock).toHaveBeenCalledWith(['a', 'b'], '{data}_{luogo}_{n:3}')
  })
})

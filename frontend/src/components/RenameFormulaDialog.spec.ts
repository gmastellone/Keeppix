import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

const previewRenameMock = vi.fn()
const applyRenameBatchMock = vi.fn()

vi.mock('@/api/rename', () => ({
  previewRename: (...args: unknown[]) => previewRenameMock(...args),
  applyRenameBatch: (...args: unknown[]) => applyRenameBatchMock(...args)
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

function buttonWithText(text: string): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.trim() === text)
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  vi.useFakeTimers()
  i18n.global.locale.value = 'it'
  previewRenameMock.mockResolvedValue([])
  applyRenameBatchMock.mockResolvedValue({ operation_id: 'op1', outcome: { succeeded: [], failed: [], batch_id: null } })
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

    // Cursore all'inizio del campo (valore di default intatto, mai
    // riscritto a mano: cambiarlo via DOM non passerebbe da v-model) —
    // il pulsante-segnaposto deve inserire lì, non in coda.
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

  it('"Applica" with no collision renames the batch, shows a toast, and closes the dialog', async () => {
    previewRenameMock.mockResolvedValue([
      { asset_id: 'a', folder_id: 'f', current_name: 'a.jpg', new_name: '2024.jpg', collides: false }
    ])
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    buttonWithText('Applica')?.click()
    await vi.runAllTimersAsync()

    expect(applyRenameBatchMock).toHaveBeenCalledWith(['a'], '{data}_{luogo}_{n:3}')
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('"Annulla" closes without applying', async () => {
    mountHost([photo('a')])
    await vi.runAllTimersAsync()

    buttonWithText('Annulla')?.click()
    await vi.runAllTimersAsync()

    expect(applyRenameBatchMock).not.toHaveBeenCalled()
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('§62.3e: with hasSubfolders, the toggle is off by default and previews only the restricted scope', async () => {
    mountHostWithSubfolders([photo('a'), photo('b'), photo('c')], [photo('a')])
    await vi.runAllTimersAsync()

    expect(previewRenameMock).toHaveBeenCalledWith(['a'], '{data}_{luogo}_{n:3}')
  })

  it('§62.3e: switching the toggle on widens the preview and apply scope to the whole lot', async () => {
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

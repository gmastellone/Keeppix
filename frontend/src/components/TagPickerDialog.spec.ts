import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

const fetchTagsMock = vi.fn()
const assignTagBatchMock = vi.fn()
const unassignTagBatchMock = vi.fn()

vi.mock('@/api/tags', () => ({
  fetchTags: (...args: unknown[]) => fetchTagsMock(...args),
  assignTagBatch: (...args: unknown[]) => assignTagBatchMock(...args),
  unassignTagBatch: (...args: unknown[]) => unassignTagBatchMock(...args)
}))

const TagPickerDialog = (await import('./TagPickerDialog.vue')).default

// Same reason as AlbumPickerDialog.spec.ts: reka-ui's `DialogPortal`
// always teleports into the real `document.body`.
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function photo(id: string, tags: TimelineAsset['tags'] = []): TimelineAsset {
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
    tags,
    faces: []
  }
}

let wrapper: VueWrapper | undefined

function mountHost(assets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheTagPickerDialog: TagPickerDialog },
    setup() {
      const open = ref(true)
      return { open, assets }
    },
    template: `<TheTagPickerDialog v-model:open="open" :assets="assets" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

function switches(): HTMLButtonElement[] {
  return Array.from(document.body.querySelectorAll('[role="switch"]'))
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
  fetchTagsMock.mockResolvedValue([
    { id: 't1', name: 'Montagna', kind: 'tag', parent_id: null, color: '#336699', assignment_count: 1 },
    { id: 't2', name: 'Mare', kind: 'tag', parent_id: null, color: null, assignment_count: 0 },
    { id: 'c1', name: 'Viaggi', kind: 'category', parent_id: null, color: null, assignment_count: 1 }
  ])
  assignTagBatchMock.mockResolvedValue(null)
  unassignTagBatchMock.mockResolvedValue(null)
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('TagPickerDialog', () => {
  it('lists only tags (not categories) from fetchTags — no per-tag fetch needed', async () => {
    mountHost([photo('a')])
    await tick()

    expect(switches()).toHaveLength(2)
    expect(document.body.textContent).toContain('Montagna')
    expect(document.body.textContent).toContain('Mare')
    expect(document.body.textContent).not.toContain('Viaggi')
    expect(fetchTagsMock).toHaveBeenCalledTimes(1)
  })

  it('a row is "on" only when every selected asset already carries the tag — derived from TimelineAsset.tags, not a fetch', async () => {
    const withTag = photo('a', [{ id: 't1', name: 'Montagna', color: '#336699', category_id: null }])
    const withoutTag = photo('b', [])
    mountHost([withTag, withoutTag])
    await tick()

    expect(switches()[0]?.getAttribute('aria-checked')).toBe('false')
  })

  it('clicking an off row assigns the tag to every selected asset in one bulk call, then flips on', async () => {
    mountHost([photo('a'), photo('b')])
    await tick()

    switches()[0]?.click()
    await tick()

    expect(assignTagBatchMock).toHaveBeenCalledWith('t1', ['a', 'b'])
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('true')
  })

  it('clicking an on row (all members) removes the tag from every selected asset, then flips off', async () => {
    const withTag = (id: string) => photo(id, [{ id: 't1', name: 'Montagna', color: '#336699', category_id: null }])
    mountHost([withTag('a'), withTag('b')])
    await tick()
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('true')

    switches()[0]?.click()
    await tick()

    expect(unassignTagBatchMock).toHaveBeenCalledWith('t1', ['a', 'b'])
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('false')
  })

  it('"Fatto" closes the dialog', async () => {
    mountHost([photo('a')])
    await tick()

    const done = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Fatto')
    done?.click()
    await tick()

    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })
})

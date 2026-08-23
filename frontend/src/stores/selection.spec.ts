import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { useSelectionStore } from './selection'

describe('selection store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('toggling the library pool never touches the culling pool', () => {
    const store = useSelectionStore()

    store.library.toggle('photo-1')
    store.library.toggle('photo-2')

    expect(store.library.selectedIds).toEqual(new Set(['photo-1', 'photo-2']))
    expect(store.culling.selectedIds).toEqual(new Set())
  })

  it('toggling the culling pool never touches the library pool', () => {
    const store = useSelectionStore()
    store.library.toggle('photo-1')

    store.culling.toggle('shot-9')

    expect(store.culling.selectedIds).toEqual(new Set(['shot-9']))
    expect(store.library.selectedIds).toEqual(new Set(['photo-1']))
  })

  it('toggle adds then removes the same id', () => {
    const store = useSelectionStore()

    store.library.toggle('photo-1')
    expect(store.library.selectedIds.has('photo-1')).toBe(true)

    store.library.toggle('photo-1')
    expect(store.library.selectedIds.has('photo-1')).toBe(false)
  })

  it('clearing one pool leaves the other pool untouched', () => {
    const store = useSelectionStore()
    store.library.toggle('photo-1')
    store.culling.toggle('shot-9')

    store.library.clear()

    expect(store.library.selectedIds).toEqual(new Set())
    expect(store.culling.selectedIds).toEqual(new Set(['shot-9']))
  })

  it('selectAllVisible adds every visible id without dropping selections made elsewhere', () => {
    const store = useSelectionStore()
    store.library.toggle('photo-outside-view')

    store.library.selectAllVisible(['photo-1', 'photo-2'])

    expect(store.library.selectedIds).toEqual(new Set(['photo-outside-view', 'photo-1', 'photo-2']))
  })

  it('selectAllVisible deselects every visible id when all of it is already selected', () => {
    const store = useSelectionStore()
    store.library.toggle('photo-outside-view')
    store.library.selectAllVisible(['photo-1', 'photo-2'])

    store.library.selectAllVisible(['photo-1', 'photo-2'])

    expect(store.library.selectedIds).toEqual(new Set(['photo-outside-view']))
  })

  it('selectAllVisible on a partially-selected visible set selects the rest, not a toggle per item', () => {
    const store = useSelectionStore()
    store.library.toggle('photo-1')

    store.library.selectAllVisible(['photo-1', 'photo-2', 'photo-3'])

    expect(store.library.selectedIds).toEqual(new Set(['photo-1', 'photo-2', 'photo-3']))
  })
})

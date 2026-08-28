import { defineStore } from 'pinia'
import { ref } from 'vue'

// Two parallel, independent selection pools: the library (Timeline,
// Favorites, Search, Albums, People) and the culling lot, which has its
// own commands and no Album/Delete. They must never talk to each other
// and never clear each other. Not a single pool with a context flag: two
// separate closures, so touching one can never, by construction, touch
// the other — verified by a test that touches one and checks the other
// stays untouched, not just that two properties exist.
function createSelectionPool() {
  const selectedIds = ref(new Set<string>())

  function toggle(id: string) {
    const next = new Set(selectedIds.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selectedIds.value = next
  }

  function clear() {
    selectedIds.value = new Set()
  }

  /** "Select all": a group toggle over what's visible — if everything
   * passed in is already selected it deselects it, otherwise it adds it
   * **without** removing photos already selected elsewhere. The label
   * never changes, even when it's currently deselecting — that's not
   * this function's job, it stays a rendering-component detail. */
  function selectAllVisible(visibleIds: string[]) {
    const allSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedIds.value.has(id))
    const next = new Set(selectedIds.value)
    if (allSelected) {
      visibleIds.forEach((id) => next.delete(id))
    } else {
      visibleIds.forEach((id) => next.add(id))
    }
    selectedIds.value = next
  }

  return { selectedIds, toggle, clear, selectAllVisible }
}

export const useSelectionStore = defineStore('selection', () => {
  const library = createSelectionPool()
  const culling = createSelectionPool()

  return { library, culling }
})

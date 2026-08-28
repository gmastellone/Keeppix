import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import type { AssetFlags, Pick as PickValue } from '@/api/culling'
import { emptySkipped, fetchFlags, pickAsset, setFlags, unvotedFlags } from '@/api/culling'
import { fetchChildren } from '@/api/folders'
import type { TimelineAsset } from '@/api/timeline'

import { useSelectionStore } from './selection'

export type CullState = 'pending' | 'taken' | 'skipped'
export type CullingLotFilter = 'all' | 'todo' | 'taken' | 'skipped'

export interface LotAsset extends TimelineAsset {
  cullState: CullState
}

/**
 * Store for the open culling lot. Entirely replaces the old
 * `stores/culling.ts`: that was a per-user voting session on "what's
 * already uploaded to the timeline", with no physical file movement — a
 * stopgap from before lot routes existed. Culling has nothing to do with
 * regular photos: it's a section that works on one specific folder and
 * physically moves photos.
 *
 * **No offline queue here**, unlike the old store: `pick` moves a file
 * for real, so a failure (permission, collision) needs to reach the user
 * immediately, not be queued and retried in the background like a simple
 * vote.
 *
 * **The "taken/skipped/pending" state comes from the folder, not a
 * re-read flag**: `set_pick` physically moves the file into
 * `_taken`/`_skipped` when the asset is inside a lot, and the route
 * returns the updated asset with its new `folder_id` — that's the source
 * of truth for `cullState`, not a second round-trip to re-read flags.
 */
export const useCullingLotStore = defineStore('cullingLot', () => {
  const lotId = ref<string | null>(null)
  const lotName = ref('')
  const assets = ref<LotAsset[]>([])
  const order = ref<string[]>([])
  const position = ref(0)
  const filter = ref<CullingLotFilter>('all')
  const flagsById = ref<Record<string, AssetFlags>>({})
  const loading = ref(false)
  const loadError = ref(false)
  const busy = ref<Set<string>>(new Set())

  // Selection pool independent from the library's (`stores/selection.ts`
  // already documents the two parallel pools: they don't talk to each
  // other and don't clear each other) — plus the shift+click/shift+arrow
  // anchor, which lives only here because it needs `order` to compute the
  // range.
  const pool = useSelectionStore().culling
  // `pool` is the reactive object exposed by the `selection` store — its
  // internal refs are already unwrapped by Pinia's proxy (same behavior
  // as `selection.library.selectedIds` elsewhere in the frontend, never
  // `.value`). A `computed` rather than capturing `pool.selectedIds`
  // once: that value gets *replaced* (not mutated) on every selection, so
  // a statically captured reference would go stale.
  const selectedIds = computed(() => pool.selectedIds)
  const selectedCount = computed(() => pool.selectedIds.size)
  const selectAnchor = ref<string | null>(null)

  const assetsById = computed(() => new Map(assets.value.map((a) => [a.id, a])))

  function flagsFor(id: string): AssetFlags {
    return flagsById.value[id] ?? unvotedFlags
  }

  function passesFilter(asset: LotAsset): boolean {
    switch (filter.value) {
      case 'todo':
        return asset.cullState === 'pending'
      case 'taken':
        return asset.cullState === 'taken'
      case 'skipped':
        return asset.cullState === 'skipped'
      case 'all':
      default:
        return true
    }
  }

  function recomputeOrder() {
    order.value = assets.value.filter(passesFilter).map((a) => a.id)
    if (position.value >= order.value.length) {
      position.value = Math.max(0, order.value.length - 1)
    }
  }

  const currentAsset = computed<LotAsset | undefined>(() => assetsById.value.get(order.value[position.value] ?? ''))

  /** The three counters are always over the **entire lot**, never the filtered queue. */
  const counts = computed(() => {
    let pending = 0
    let taken = 0
    let skipped = 0
    for (const asset of assets.value) {
      if (asset.cullState === 'pending') pending += 1
      else if (asset.cullState === 'taken') taken += 1
      else skipped += 1
    }
    return { pending, taken, skipped }
  })

  /**
   * Builds a lot's list from at most three calls: the lot's direct
   * (pending) assets, plus, if they already exist, the `_taken`/`_skipped`
   * children (`ensure_culling_child` only creates them on the first
   * "Take"/"Skip" — they often don't exist yet). No need to invent a
   * "give me the whole lot" route: this is the same composition the open
   * lot would need anyway to show its three queues.
   */
  async function open(id: string, name: string): Promise<void> {
    lotId.value = id
    lotName.value = name
    loading.value = true
    loadError.value = false
    filter.value = 'all'
    position.value = 0
    flagsById.value = {}
    clearSelection()
    try {
      const root = await fetchChildren(id)
      const pending: LotAsset[] = root.assets.map((a) => ({ ...a, cullState: 'pending' as const }))
      const takenFolder = root.folders.find((f) => f.name === '_taken')
      const skippedFolder = root.folders.find((f) => f.name === '_skipped')
      const [takenChildren, skippedChildren] = await Promise.all([
        takenFolder ? fetchChildren(takenFolder.id) : Promise.resolve(null),
        skippedFolder ? fetchChildren(skippedFolder.id) : Promise.resolve(null)
      ])
      const taken: LotAsset[] = (takenChildren?.assets ?? []).map((a) => ({ ...a, cullState: 'taken' as const }))
      const skipped: LotAsset[] = (skippedChildren?.assets ?? []).map((a) => ({ ...a, cullState: 'skipped' as const }))
      assets.value = [...pending, ...taken, ...skipped]
      recomputeOrder()
    } catch {
      assets.value = []
      order.value = []
      loadError.value = true
    } finally {
      loading.value = false
    }
  }

  function setFilter(next: CullingLotFilter) {
    filter.value = next
    position.value = 0
    recomputeOrder()
  }

  function goTo(delta: number) {
    if (order.value.length === 0) return
    position.value = Math.min(Math.max(position.value + delta, 0), order.value.length - 1)
  }

  function goToId(id: string) {
    const idx = order.value.indexOf(id)
    if (idx >= 0) position.value = idx
  }

  /** The actual round-trip for a single photo: HTTP call plus local
   * update. Shared by `decide` (which picks `nextPick` using the toggle
   * rule) and `decideMany` (which forces it, without toggling — "move all
   * selected to taken/skipped"). */
  async function applyPick(assetId: string, nextPick: PickValue): Promise<boolean> {
    const asset = assetsById.value.get(assetId)
    if (!asset || busy.value.has(assetId)) return false
    busy.value = new Set(busy.value).add(assetId)
    try {
      const updated = await pickAsset(assetId, nextPick)
      const nextState: CullState = nextPick === 'none' ? 'pending' : nextPick === 'pick' ? 'taken' : 'skipped'
      assets.value = assets.value.map((a) =>
        a.id === assetId ? { ...a, ...updated, cullState: nextState } : a
      )
      if (assetId in flagsById.value) {
        flagsById.value = { ...flagsById.value, [assetId]: { ...flagsFor(assetId), pick: nextPick } }
      }
      return true
    } catch {
      return false
    } finally {
      const next = new Set(busy.value)
      next.delete(assetId)
      busy.value = next
    }
  }

  /**
   * "Take"/"Skip" (`decideCulling`): a click behaves like a toggle
   * label, never a dialog. Repeating the same decision undoes it (back
   * to `pending`); deciding the opposite goes straight from one state to
   * the other, without passing through `pending`.
   */
  async function decide(assetId: string, target: 'taken' | 'skipped'): Promise<void> {
    const asset = assetsById.value.get(assetId)
    if (!asset) return
    const nextPick: PickValue = asset.cullState === target ? 'none' : target === 'taken' ? 'pick' : 'reject'
    const ok = await applyPick(assetId, nextPick)
    if (ok) recomputeOrder()
  }

  /** Selection bar bulk "Take"/"Skip": unlike `decide`, this **forces**
   * the target state instead of toggling — photos already in that state
   * are skipped over (they still count as succeeded, since there's
   * nothing to fix). Partial success is allowed, same as "Empty
   * skipped": whatever fails stays where it was. */
  async function decideMany(ids: string[], target: 'taken' | 'skipped'): Promise<{ succeeded: number; failed: number }> {
    let succeeded = 0
    let failed = 0
    for (const id of ids) {
      const asset = assetsById.value.get(id)
      if (!asset) continue
      if (asset.cullState === target) {
        succeeded++
        continue
      }
      const nextPick: PickValue = target === 'taken' ? 'pick' : 'reject'
      const ok = await applyPick(id, nextPick)
      if (ok) succeeded++
      else failed++
    }
    recomputeOrder()
    return { succeeded, failed }
  }

  function clearSelection() {
    pool.clear()
    selectAnchor.value = null
  }

  /** Range `[a..b]` within `order`, regardless of which order the
   * endpoints are passed in — the same shape no matter which control
   * requested it (filmstrip, keyboard). */
  function rangeIds(fromId: string, toId: string): string[] {
    const from = order.value.indexOf(fromId)
    const to = order.value.indexOf(toId)
    if (from === -1 || to === -1) return [toId]
    const [start, end] = from <= to ? [from, to] : [to, from]
    return order.value.slice(start, end + 1)
  }

  /** Thumbnail checkbox, no shift: toggles that single photo and moves
   * the anchor onto it. */
  function toggleSelect(id: string) {
    pool.toggle(id)
    selectAnchor.value = id
  }

  /** Shift+click on the thumbnail body: always selects the entire
   * `[anchor..id]` range, replacing the previous selection — never
   * additive. Without an anchor, uses the currently open photo; the
   * anchor stays whatever's already in use once set. */
  function selectRangeToThumb(id: string) {
    const anchor = selectAnchor.value ?? currentAsset.value?.id ?? id
    selectAnchor.value = anchor
    pool.selectedIds = new Set(rangeIds(anchor, id))
  }

  /** Shift+click on the checkbox: same as above, but only if an anchor
   * already exists; otherwise behaves like a plain click. */
  function selectRangeOrToggle(id: string) {
    if (!selectAnchor.value) {
      toggleSelect(id)
      return
    }
    pool.selectedIds = new Set(rangeIds(selectAnchor.value, id))
  }

  /** Keyboard shift+arrow: the anchor is whatever's already in use, or
   * the current photo; the index moves by `delta`, then the entire
   * `[anchor..new index]` range is recalculated from scratch. */
  function selectRangeByArrow(delta: number) {
    if (order.value.length === 0) return
    const anchor = selectAnchor.value ?? currentAsset.value?.id ?? order.value[position.value]
    selectAnchor.value = anchor
    position.value = Math.min(Math.max(position.value + delta, 0), order.value.length - 1)
    const newId = order.value[position.value]
    pool.selectedIds = new Set(rangeIds(anchor, newId))
  }

  /** "Select all" icon (outside the selection bar): adds the entire
   * current queue and clears the anchor. */
  function selectAllInQueue() {
    pool.selectedIds = new Set([...pool.selectedIds, ...order.value])
    selectAnchor.value = null
  }

  /** "Select all" inside the selection bar: a toggle over the current
   * queue, not a plain "add everything". */
  function toggleSelectAllInQueue() {
    pool.selectAllVisible(order.value)
  }

  /** Fetches flags (rating, favorite) only for the requested asset —
   * never for the whole lot: a lot can have hundreds of photos, and
   * there's no bulk-read route (`POST /flags/batch` is write-only).
   * Called when each photo opens, not when the lot loads. */
  async function ensureFlagsLoaded(assetId: string): Promise<void> {
    if (assetId in flagsById.value) return
    try {
      const remote = await fetchFlags(assetId)
      flagsById.value = { ...flagsById.value, [assetId]: remote }
    } catch {
      // silent: rating stays at 0 stars until retried
    }
  }

  /** Star rating / keys `1`-`5`: pressing the same number again clears
   * it. Not optimistic: reads current flags before writing, so it
   * doesn't overwrite `favorite`/`color_label` with a partial
   * replacement. */
  async function setRating(assetId: string, rating: number): Promise<void> {
    const asset = assetsById.value.get(assetId)
    if (!asset || busy.value.has(assetId)) return
    await ensureFlagsLoaded(assetId)
    const current = flagsFor(assetId)
    const next = current.rating === rating ? 0 : rating
    const patch: AssetFlags = { ...current, rating: next === 0 ? null : next }
    busy.value = new Set(busy.value).add(assetId)
    try {
      await setFlags(assetId, patch)
      flagsById.value = { ...flagsById.value, [assetId]: patch }
    } finally {
      const rest = new Set(busy.value)
      rest.delete(assetId)
      busy.value = rest
    }
  }

  /** Favorite (lightbox, `@toggle-favorite`): an axis independent from
   * `cullState`, same principle as `stores/favorites.ts`. */
  async function toggleFavorite(assetId: string): Promise<void> {
    await ensureFlagsLoaded(assetId)
    const current = flagsFor(assetId)
    const patch: AssetFlags = { ...current, favorite: !current.favorite }
    await setFlags(assetId, patch)
    flagsById.value = { ...flagsById.value, [assetId]: patch }
    assets.value = assets.value.map((a) => (a.id === assetId ? { ...a, favorite: patch.favorite } : a))
  }

  /** "Empty skipped": permanent deletion, partial success allowed — an
   * asset whose purge fails stays in the lot, it doesn't disappear
   * halfway. */
  async function emptySkippedFolder(): Promise<{ succeeded: number; failed: number }> {
    if (!lotId.value) return { succeeded: 0, failed: 0 }
    const outcome = await emptySkipped(lotId.value)
    const removed = new Set(outcome.succeeded)
    assets.value = assets.value.filter((a) => !removed.has(a.id))
    filter.value = 'all'
    position.value = 0
    recomputeOrder()
    return { succeeded: outcome.succeeded.length, failed: outcome.failed.length }
  }

  return {
    lotId,
    lotName,
    assets,
    order,
    position,
    filter,
    loading,
    loadError,
    busy,
    currentAsset,
    counts,
    selectedIds,
    selectedCount,
    open,
    setFilter,
    goTo,
    goToId,
    decide,
    decideMany,
    setRating,
    ensureFlagsLoaded,
    toggleFavorite,
    emptySkippedFolder,
    flagsFor,
    clearSelection,
    toggleSelect,
    selectRangeToThumb,
    selectRangeOrToggle,
    selectRangeByArrow,
    selectAllInQueue,
    toggleSelectAllInQueue
  }
})

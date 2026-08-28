import { defineStore } from 'pinia'
import { ref } from 'vue'

import { fetchFlags, setFlags, unvotedFlags } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

import { useToastStore } from './toast'

/**
 * The favorite heart toggle: adds/removes from favorites, "immediately,
 * with no confirmation or toast" for a single tap on a thumbnail; with a
 * neutral toast only for the selection bar's bulk action.
 *
 * `PUT /assets/{id}/flags` is a **full replacement** of the vote, not a
 * patch (see the comment on `AssetFlagsBody::favorite` in
 * crates/keeppix-api/src/routes/flags.rs) — writing `favorite` alone
 * would silently clear any existing `rating`/`pick`/`color_label`. That's
 * why every write here first reads the asset's current flags: the
 * timeline/Favorites views don't keep them in memory (they aren't part of
 * `TimelineAsset`), unlike the culling session, which keeps them all
 * preloaded.
 */
export const useFavoritesStore = defineStore('favorites', () => {
  /** Optimistic overrides on top of `TimelineAsset.favorite` (the
   * snapshot the list was loaded with) — only for ids touched in this
   * browsing session, never persisted. */
  const overlay = ref<Record<string, boolean>>({})

  function isFavorite(asset: TimelineAsset): boolean {
    return overlay.value[asset.id] ?? asset.favorite
  }

  async function writeFavorite(assetId: string, value: boolean): Promise<boolean> {
    try {
      const current = await fetchFlags(assetId).catch(() => unvotedFlags)
      await setFlags(assetId, { ...current, favorite: value })
      return true
    } catch {
      return false
    }
  }

  /** Single favorite toggle: optimistic, no toast — if the write fails
   * the optimistic update is undone and only then does an error appear,
   * consistent with the error-handling discipline already in use
   * elsewhere in the app. */
  async function toggleOne(asset: TimelineAsset): Promise<void> {
    const next = !isFavorite(asset)
    overlay.value = { ...overlay.value, [asset.id]: next }
    const ok = await writeFavorite(asset.id, next)
    if (!ok) {
      overlay.value = { ...overlay.value, [asset.id]: !next }
      useToastStore().showError(i18n.global.t('ui.favorites.toggleError'))
    }
  }

  /** Selection bar bulk action: the add/remove toggle is decided by the
   * caller (it depends on the state of *the whole* selection, which this
   * store doesn't know), this only applies it sequentially — same
   * principle as `removeMany` in the culling store: a library selection
   * is typically dozens or hundreds of photos, not thousands, and
   * "one at a time" ordering matters more than parallelizing. */
  async function setMany(assets: TimelineAsset[], value: boolean): Promise<void> {
    if (assets.length === 0) return
    const previous = { ...overlay.value }
    overlay.value = {
      ...overlay.value,
      ...Object.fromEntries(assets.map((asset) => [asset.id, value]))
    }
    const failed: TimelineAsset[] = []
    for (const asset of assets) {
      const ok = await writeFavorite(asset.id, value)
      if (!ok) {
        failed.push(asset)
        overlay.value = { ...overlay.value, [asset.id]: previous[asset.id] ?? asset.favorite }
      }
    }
    const toast = useToastStore()
    const okCount = assets.length - failed.length
    if (failed.length === 0) {
      toast.show(i18n.global.t(value ? 'ui.selectionBar.favoritesAdded' : 'ui.selectionBar.favoritesRemoved'))
    } else if (okCount > 0) {
      toast.showPartial(okCount, failed.length, () => void setMany(failed, value))
    } else {
      toast.showError(i18n.global.t('ui.favorites.toggleError'))
    }
  }

  return { overlay, isFavorite, toggleOne, setMany }
})

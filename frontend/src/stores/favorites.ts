import { defineStore } from 'pinia'
import { ref } from 'vue'

import { fetchFlags, setFlags, unvotedFlags } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

import { useToastStore } from './toast'

/**
 * Il cuoricino di SP-1/SP-2 (documento funzionale §8.4, §10.3, §12.3):
 * aggiunge/rimuove dai preferiti, "subito, senza conferma né toast" per il
 * singolo tocco sulla tessera; con toast neutro solo per l'azione di
 * gruppo della barra di selezione.
 *
 * `PUT /assets/{id}/flags` è un **rimpiazzo completo** del voto, non una
 * patch (commento di `AssetFlagsBody::favorite` in
 * crates/keeppix-api/src/routes/flags.rs) — scrivere `favorite` da solo
 * azzererebbe silenziosamente `rating`/`pick`/`color_label` già presenti.
 * Per questo ogni scrittura qui legge prima i flag correnti dell'asset:
 * la timeline/Preferiti non li tengono in memoria (non fanno parte di
 * `TimelineAsset`), a differenza della sessione di culling che li tiene
 * tutti già caricati.
 */
export const useFavoritesStore = defineStore('favorites', () => {
  /** Sovrascritture ottimistiche rispetto a `TimelineAsset.favorite`
   * (l'istantanea con cui la lista è stata caricata) — solo per gli id
   * toccati in questa sessione di navigazione, mai persistito. */
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

  /** Cuoricino singolo (SP-1): ottimistico, nessun toast — se la scrittura
   * fallisce l'ottimismo viene disfatto e solo lì compare un errore, non
   * previsto dal prototipo (che è client-side puro) ma coerente con la
   * disciplina di errore già in uso altrove nell'app. */
  async function toggleOne(asset: TimelineAsset): Promise<void> {
    const next = !isFavorite(asset)
    overlay.value = { ...overlay.value, [asset.id]: next }
    const ok = await writeFavorite(asset.id, next)
    if (!ok) {
      overlay.value = { ...overlay.value, [asset.id]: !next }
      useToastStore().showError(i18n.global.t('ui.favorites.toggleError'))
    }
  }

  /** Azione di gruppo della barra di selezione (§12.3): il toggle
   * add/remove è deciso dal chiamante (dipende dallo stato di *tutta* la
   * selezione, che questo store non conosce), qui solo l'applicazione
   * sequenziale — stesso principio di `removeMany` nel culling store: una
   * selezione di libreria è tipicamente decine o centinaia di foto, non
   * migliaia, e l'ordine "una alla volta" conta più della parallelizzazione. */
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

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
 * Store del lotto di culling aperto (documento funzionale §15, Fase 11
 * Task 17). Sostituisce interamente `stores/culling.ts` (Task 17 4/N la
 * rimuove): quella era una sessione a voto per-utente su "quanto già
 * caricato in timeline", senza spostamento fisico — un ripiego prima che
 * le rotte dei lotti esistessero (Ruling, 24 agosto 2026: "il culling non
 * c'entra più nulla con le foto normali, è una sezione che lavora solo su
 * una cartella specifica e sposta fisicamente le foto").
 *
 * **Nessuna coda offline qui**, a differenza del vecchio store: `pick`
 * sposta un file per davvero, un fallimento (permesso, collisione) deve
 * arrivare all'utente subito, non essere accodato e ritentato in
 * background come un semplice voto.
 *
 * **Lo stato "preso/scartato/da vedere" viene dalla cartella, non da un
 * flag riletto**: `set_pick` (Fase 9 Task 4) sposta fisicamente il file in
 * `_taken`/`_skipped` quando l'asset è dentro un lotto, e la rotta
 * restituisce l'asset aggiornato con il nuovo `folder_id` — è quella la
 * fonte di verità per `cullState`, non un secondo giro per rileggere i
 * flag.
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

  // Task 17 (4/N), §15.3 controlli 20-25: pool di selezione indipendente da
  // quello della libreria (stores/selection.ts lo dichiara già dei due pool
  // paralleli, "non si parlano e non si azzerano a vicenda") — più
  // l'ancora dello shift+click/shift+freccia, che vive solo qui perché
  // serve `order` per calcolare l'intervallo.
  const pool = useSelectionStore().culling
  // `pool` è l'oggetto reattivo esposto dallo store `selection` — i suoi
  // ref interni sono già spacchettati dal proxy di Pinia (stesso
  // comportamento di `selection.library.selectedIds` altrove nel
  // frontend, mai `.value`). Un `computed` invece di catturare
  // `pool.selectedIds` una volta sola: quel valore viene *rimpiazzato*
  // (non mutato) a ogni selezione, un riferimento catturato staticamente
  // andrebbe stantio.
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

  /** §15.A.3: i tre contatori sono sempre sul **lotto intero**, mai sulla coda filtrata. */
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
   * Compone la lista di un lotto da tre chiamate al più: gli asset diretti
   * del lotto (in attesa) più, se esistono già, i figli `_taken`/`_skipped`
   * (`ensure_culling_child` li crea solo al primo "Scelta"/"Scarta" —
   * spesso non esistono ancora). Nessuna rotta "dammi tutto il lotto" da
   * inventare: la stessa composizione che il lotto aperto farebbe comunque
   * per mostrare le tre code.
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

  /** Il giro reale su una sola foto: chiamata HTTP + aggiornamento locale.
   * Condiviso fra `decide` (che sceglie `nextPick` con la regola del
   * toggle) e `decideMany` (che lo forza, senza toggle — §15.3 controlli
   * 23-24, "Porta tutte le selezionate a taken/skipped"). */
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
   * "Scelta"/"Scarta" (`decideCulling`): click identico a un'etichetta,
   * mai un dialog. Ripetere la stessa decisione la annulla (torna
   * `pending`); decidere l'opposto passa direttamente da uno stato
   * all'altro, senza transitare per `pending`.
   */
  async function decide(assetId: string, target: 'taken' | 'skipped'): Promise<void> {
    const asset = assetsById.value.get(assetId)
    if (!asset) return
    const nextPick: PickValue = asset.cullState === target ? 'none' : target === 'taken' ? 'pick' : 'reject'
    const ok = await applyPick(assetId, nextPick)
    if (ok) recomputeOrder()
  }

  /** Barra di selezione, "Scelta"/"Scarta" di massa (§15.3 controlli 23-24):
   * a differenza di `decide`, **forza** lo stato target invece di fare
   * toggle — foto già a quello stato vengono saltate (contano comunque fra
   * le riuscite, non c'è nulla da correggere). Riuscita parziale come
   * "Svuota scartati": chi fallisce resta dov'era. */
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

  /** Intervallo `[a..b]` dentro `order`, in qualunque verso siano passati
   * gli estremi — la stessa forma qualunque sia il controllo che l'ha
   * chiesta (filmino, tastiera). */
  function rangeIds(fromId: string, toId: string): string[] {
    const from = order.value.indexOf(fromId)
    const to = order.value.indexOf(toId)
    if (from === -1 || to === -1) return [toId]
    const [start, end] = from <= to ? [from, to] : [to, from]
    return order.value.slice(start, end + 1)
  }

  /** Checkbox della miniatura, senza shift: inverte quella sola foto e
   * sposta l'ancora su di essa (§15.4, "Click sulla checkbox"). */
  function toggleSelect(id: string) {
    pool.toggle(id)
    selectAnchor.value = id
  }

  /** Shift+click sul corpo della miniatura: seleziona sempre l'intero
   * intervallo `[ancora..id]`, sostituendo la selezione precedente — mai
   * additiva (§15.5, "non si sommano a una selezione precedente, si
   * sostituiscono ad essa"). Senza ancora, usa la foto aperta; l'ancora
   * resta quella già in uso una volta impostata. */
  function selectRangeToThumb(id: string) {
    const anchor = selectAnchor.value ?? currentAsset.value?.id ?? id
    selectAnchor.value = anchor
    pool.selectedIds = new Set(rangeIds(anchor, id))
  }

  /** Shift+click sulla checkbox: come sopra, ma solo se un'ancora esiste
   * già; altrimenti si comporta come un click semplice (§15.4). */
  function selectRangeOrToggle(id: string) {
    if (!selectAnchor.value) {
      toggleSelect(id)
      return
    }
    pool.selectedIds = new Set(rangeIds(selectAnchor.value, id))
  }

  /** Shift+freccia da tastiera (§15.5): l'ancora è quella già in uso o la
   * foto corrente; l'indice si sposta di `delta`, poi l'intero intervallo
   * `[ancora..nuovo indice]` viene ricalcolato da zero. */
  function selectRangeByArrow(delta: number) {
    if (order.value.length === 0) return
    const anchor = selectAnchor.value ?? currentAsset.value?.id ?? order.value[position.value]
    selectAnchor.value = anchor
    position.value = Math.min(Math.max(position.value + delta, 0), order.value.length - 1)
    const newId = order.value[position.value]
    pool.selectedIds = new Set(rangeIds(anchor, newId))
  }

  /** Icona "Seleziona tutto" (§15.3 controllo 8, fuori dalla barra di
   * selezione): aggiunge tutta la coda corrente e azzera l'ancora. */
  function selectAllInQueue() {
    pool.selectedIds = new Set([...pool.selectedIds, ...order.value])
    selectAnchor.value = null
  }

  /** "Seleziona tutte" dentro la barra di selezione (§15.3 controllo 22):
   * interruttore sulla coda corrente, non un semplice "aggiungi tutto". */
  function toggleSelectAllInQueue() {
    pool.selectAllVisible(order.value)
  }

  /** Recupera i flag (valutazione, preferito) solo per l'asset richiesto —
   * mai per l'intero lotto: un lotto può avere centinaia di foto, e non
   * esiste una rotta di lettura in blocco (`POST /flags/batch` è solo
   * scrittura). Chiamata all'apertura di ogni foto, non a caricamento del
   * lotto. */
  async function ensureFlagsLoaded(assetId: string): Promise<void> {
    if (assetId in flagsById.value) return
    try {
      const remote = await fetchFlags(assetId)
      flagsById.value = { ...flagsById.value, [assetId]: remote }
    } catch {
      // silenzioso: la valutazione resta a 0 stelle finché non si riprova
    }
  }

  /** Stelle / tasti `1`-`5`: ripremere lo stesso numero azzera (SP-20). Non
   * ottimistico: legge i flag correnti prima di scrivere, così non
   * sovrascrive `favorite`/`color_label` con un rimpiazzo parziale. */
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

  /** Preferito (lightbox, `@toggle-favorite`): asse indipendente da
   * `cullState`, stesso principio di `stores/favorites.ts`. */
  async function toggleFavorite(assetId: string): Promise<void> {
    await ensureFlagsLoaded(assetId)
    const current = flagsFor(assetId)
    const patch: AssetFlags = { ...current, favorite: !current.favorite }
    await setFlags(assetId, patch)
    flagsById.value = { ...flagsById.value, [assetId]: patch }
    assets.value = assets.value.map((a) => (a.id === assetId ? { ...a, favorite: patch.favorite } : a))
  }

  /** "Svuota scartati": cancellazione definitiva, riuscita parziale — un
   * asset il cui purge fallisce resta nel lotto, non sparisce a metà. */
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

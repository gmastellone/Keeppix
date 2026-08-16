import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { deleteAsset, fetchFlags, setFlags, unvotedFlags } from '@/api/culling'
import type { AssetFlags, DiskAction, Pick } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'

export type CullingFilter = 'all' | 'pending' | 'picks' | 'rejects'

export interface QueuedVote {
  assetId: string
  flags: AssetFlags
}

/**
 * Attesa fra un tentativo fallito e il prossimo. Non esponenziale: la spec
 * chiede "si accodano e si ritentano", non un backoff — una sessione di
 * culling dura minuti, non ore, quindi un intervallo fisso e breve è
 * sufficiente e più semplice da ragionare.
 */
const RETRY_DELAY_MS = 4000

/**
 * Store della sessione di culling (spec §4.2). Tiene l'insieme di lavoro
 * (`assets`), i voti dell'utente corrente (`flagsById`, ottimistici: si
 * aggiornano subito in locale, la rete insegue), e l'ordine di navigazione
 * filtrato (`order`/`position`).
 *
 * `order` è ricalcolato solo in `start()` e `setFilter()`, mai dentro
 * `vote()`: cambiare filtro a ogni voto farebbe "saltare" la foto corrente
 * fuori dalla vista nel mezzo dell'azione da tastiera. Il filtro si applica
 * quando l'utente lo sceglie esplicitamente, non come effetto collaterale.
 */
export const useCullingStore = defineStore('culling', () => {
  const assets = ref<TimelineAsset[]>([])
  const flagsById = ref<Record<string, AssetFlags>>({})
  const order = ref<string[]>([])
  const position = ref(0)
  const filter = ref<CullingFilter>('all')
  const queue = ref<QueuedVote[]>([])
  const flushing = ref(false)
  const compareMode = ref(false)
  const zoomed = ref(false)

  let retryTimer: ReturnType<typeof setTimeout> | undefined
  let onlineBound = false
  const hydrated = new Set<string>()

  const assetsById = computed(() => new Map(assets.value.map((a) => [a.id, a])))

  function flagsFor(id: string): AssetFlags {
    return flagsById.value[id] ?? unvotedFlags
  }

  function passesFilter(id: string): boolean {
    const pick: Pick = flagsFor(id).pick
    switch (filter.value) {
      case 'pending':
        return pick === 'none'
      case 'picks':
        return pick === 'pick'
      case 'rejects':
        return pick === 'reject'
      case 'all':
      default:
        return true
    }
  }

  function recomputeOrder(keepCurrent: boolean) {
    const currentId = order.value[position.value]
    order.value = assets.value.map((a) => a.id).filter(passesFilter)
    if (keepCurrent && currentId && order.value.includes(currentId)) {
      position.value = order.value.indexOf(currentId)
    } else {
      position.value = 0
    }
  }

  /** Avvia una sessione di culling su un nuovo insieme di lavoro. */
  function start(list: TimelineAsset[], initialFlags: Record<string, AssetFlags> = {}) {
    assets.value = list
    flagsById.value = initialFlags
    filter.value = 'all'
    compareMode.value = false
    zoomed.value = false
    queue.value = []
    hydrated.clear()
    recomputeOrder(false)
    bindOnline()
  }

  function bindOnline() {
    if (onlineBound || typeof window === 'undefined') return
    window.addEventListener('online', () => void retryQueue())
    onlineBound = true
  }

  function stop() {
    if (retryTimer) {
      clearTimeout(retryTimer)
      retryTimer = undefined
    }
  }

  function setFilter(next: CullingFilter) {
    filter.value = next
    recomputeOrder(true)
  }

  const currentAsset = computed<TimelineAsset | undefined>(() =>
    assetsById.value.get(order.value[position.value] ?? '')
  )

  const total = computed(() => assets.value.length)
  const pendingCount = computed(() => assets.value.filter((a) => flagsFor(a.id).pick === 'none').length)
  const pickCount = computed(() => assets.value.filter((a) => flagsFor(a.id).pick === 'pick').length)
  const rejectCount = computed(() => assets.value.filter((a) => flagsFor(a.id).pick === 'reject').length)

  function goTo(delta: number) {
    if (order.value.length === 0) return
    position.value = Math.min(Math.max(position.value + delta, 0), order.value.length - 1)
  }

  function advance() {
    goTo(1)
  }

  function goToId(id: string) {
    const idx = order.value.indexOf(id)
    if (idx >= 0) position.value = idx
  }

  function applyLocalFlags(assetId: string, patch: Partial<AssetFlags>) {
    const current = flagsFor(assetId)
    flagsById.value = { ...flagsById.value, [assetId]: { ...current, ...patch } }
  }

  function enqueue(entry: QueuedVote) {
    const idx = queue.value.findIndex((v) => v.assetId === entry.assetId)
    if (idx >= 0) {
      queue.value = [...queue.value.slice(0, idx), entry, ...queue.value.slice(idx + 1)]
    } else {
      queue.value = [...queue.value, entry]
    }
    void flushQueue()
  }

  async function flushQueue(): Promise<void> {
    if (flushing.value) return
    flushing.value = true
    try {
      // Un voto alla volta, nell'ordine in cui sono arrivati: se il primo
      // fallisce si ritenta lui, non si passa al successivo scavalcandolo.
      while (queue.value.length > 0) {
        const next = queue.value[0]
        try {
          await setFlags(next.assetId, next.flags)
          queue.value = queue.value.slice(1)
        } catch {
          scheduleRetry()
          break
        }
      }
    } finally {
      flushing.value = false
    }
  }

  function scheduleRetry() {
    if (retryTimer) return
    retryTimer = setTimeout(() => {
      retryTimer = undefined
      void flushQueue()
    }, RETRY_DELAY_MS)
  }

  /** Ritenta subito la coda, cancellando un eventuale timer già programmato. */
  async function retryQueue(): Promise<void> {
    if (retryTimer) {
      clearTimeout(retryTimer)
      retryTimer = undefined
    }
    await flushQueue()
  }

  function vote(assetId: string, patch: Partial<AssetFlags>) {
    applyLocalFlags(assetId, patch)
    enqueue({ assetId, flags: flagsFor(assetId) })
    if (assetId === currentAsset.value?.id) {
      advance()
    }
  }

  function rate(assetId: string, rating: number) {
    vote(assetId, { rating })
  }

  /** `p`: alterna pick/nessun voto. Premerlo due volte annulla la scelta. */
  function pick(assetId: string) {
    const current = flagsFor(assetId).pick === 'pick' ? 'none' : 'pick'
    vote(assetId, { pick: current })
  }

  /** `x`: alterna reject/nessun voto, stessa logica di `pick`. */
  function reject(assetId: string) {
    const current = flagsFor(assetId).pick === 'reject' ? 'none' : 'reject'
    vote(assetId, { pick: current })
  }

  function toggleZoom() {
    zoomed.value = !zoomed.value
  }

  function toggleCompare() {
    compareMode.value = !compareMode.value
  }

  /**
   * Applica una delle tre opzioni di cancellazione (spec §6) e toglie
   * l'asset dall'insieme di lavoro. Non ottimistica: se la richiesta fallisce
   * l'asset resta nella sessione, niente sparisce senza conferma dal server.
   */
  async function remove(assetId: string, diskAction: DiskAction): Promise<void> {
    await deleteAsset(assetId, diskAction)
    const wasCurrent = assetId === currentAsset.value?.id
    assets.value = assets.value.filter((a) => a.id !== assetId)
    flagsById.value = Object.fromEntries(
      Object.entries(flagsById.value).filter(([id]) => id !== assetId)
    )
    recomputeOrder(!wasCurrent)
  }

  /**
   * Sequenziale di proposito: un batch nel culling è tipicamente qualche
   * decina di scarti, non le migliaia dell'editing batch dei metadati — la
   * semplicità di "uno alla volta, nell'ordine" conta più della parallelizzazione.
   */
  async function removeMany(assetIds: string[], diskAction: DiskAction): Promise<void> {
    for (const id of assetIds) {
      await remove(id, diskAction)
    }
  }

  /**
   * Recupera i voti già presenti sul server la prima volta che un asset
   * entra in sessione (es. una foto già votata in una sessione precedente).
   * Non sovrascrive mai un voto locale già in coda: la coda vince sempre sul
   * valore remoto, altrimenti un voto appena dato sparirebbe se il fetch
   * arriva dopo.
   */
  async function ensureFlagsLoaded(assetId: string): Promise<void> {
    if (hydrated.has(assetId)) return
    hydrated.add(assetId)
    try {
      const remote = await fetchFlags(assetId)
      const alreadyQueued = queue.value.some((v) => v.assetId === assetId)
      const alreadyKnownLocally = assetId in flagsById.value
      if (!alreadyQueued && !alreadyKnownLocally) {
        flagsById.value = { ...flagsById.value, [assetId]: remote }
      }
    } catch {
      hydrated.delete(assetId)
    }
  }

  return {
    assets,
    flagsById,
    order,
    position,
    filter,
    queue,
    compareMode,
    zoomed,
    currentAsset,
    total,
    pendingCount,
    pickCount,
    rejectCount,
    start,
    stop,
    setFilter,
    goTo,
    advance,
    goToId,
    vote,
    rate,
    pick,
    reject,
    toggleZoom,
    toggleCompare,
    remove,
    removeMany,
    flagsFor,
    retryQueue,
    flushQueue,
    ensureFlagsLoaded
  }
})

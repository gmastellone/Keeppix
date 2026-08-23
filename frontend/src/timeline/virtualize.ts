/**
 * Virtualizzatore scritto in casa (Fase 11 Task 4, Ruling della spec
 * fase-11-interfaccia.md §2): somme prefisse delle altezze di riga più
 * ricerca binaria su `scrollTop`. Nessuna libreria — un virtualizzatore a
 * misurazione risolverebbe un problema che qui non esiste, perché la
 * geometria (Fase 10) dà larghezza e altezza di ogni scatto *prima* di
 * disegnare: le altezze di riga sono calcolabili in anticipo con
 * esattezza, non misurate riga per riga mentre entrano nella finestra.
 *
 * Agnostico rispetto a cosa *sia* una riga (griglia di foto, intestazione
 * di mese): riceve solo un array di altezze in pixel, nello stesso ordine
 * in cui le righe vanno disegnate dall'alto verso il basso.
 */

export interface VisibleRange {
  /** Indice della prima riga da montare (inclusivo). */
  start: number
  /** Indice oltre l'ultima riga da montare (esclusivo, come `Array.slice`). */
  end: number
}

export class RowVirtualizer {
  /** Altezza totale del contenuto: dà alla barra di scorrimento la sua
   * lunghezza vera fin dal primo istante (documento funzionale §66.2),
   * senza dover montare una sola riga. */
  readonly totalHeight: number

  private readonly heights: readonly number[]
  /** `prefix[i]` = somma delle altezze di tutte le righe prima di `i`.
   * `prefix.length === heights.length + 1`; `prefix[heights.length] ===
   * totalHeight`. */
  private readonly prefix: Float64Array

  constructor(rowHeights: readonly number[]) {
    this.heights = rowHeights
    const prefix = new Float64Array(rowHeights.length + 1)
    for (let i = 0; i < rowHeights.length; i++) {
      prefix[i + 1] = prefix[i] + rowHeights[i]
    }
    this.prefix = prefix
    this.totalHeight = prefix[rowHeights.length] ?? 0
  }

  get rowCount(): number {
    return this.heights.length
  }

  /** Coordinata `y` (in pixel, dall'alto del contenuto) a cui inizia la riga. */
  rowTop(index: number): number {
    return this.prefix[index] ?? this.totalHeight
  }

  rowHeight(index: number): number {
    return this.heights[index] ?? 0
  }

  /**
   * Indice della riga la cui banda `[top, top+height)` contiene `y` — o
   * l'ultima riga se `y` cade oltre la fine del contenuto. Ricerca binaria
   * sulle somme prefisse, `O(log n)`: è il nucleo di `visibleRange`, insieme
   * a `firstRowStartingAtOrAfter` qui sotto.
   */
  private rowAtOffset(y: number): number {
    if (this.heights.length === 0) return 0
    let lo = 0
    let hi = this.heights.length // esclusivo
    while (lo < hi) {
      const mid = (lo + hi) >>> 1
      if (this.prefix[mid + 1]! <= y) lo = mid + 1
      else hi = mid
    }
    return Math.min(lo, this.heights.length - 1)
  }

  /**
   * Indice della prima riga che comincia a `y` o oltre — l'estremo
   * superiore esclusivo dell'intervallo di righe che iniziano prima di `y`.
   * Non è lo stesso calcolo di `rowAtOffset`: su un confine esatto fra due
   * righe (`y` uguale esattamente a un `rowTop`), `rowAtOffset(y)`
   * risponde con la riga che *comincia* lì (convenzione a intervallo
   * semiaperto `[top, bottom)`), ma quella riga non si sovrappone a
   * `[…, y)` — usare `rowAtOffset(to) + 1` come limite superiore
   * includerebbe una riga di troppo esattamente su quel confine.
   */
  private firstRowStartingAtOrAfter(y: number): number {
    let lo = 0
    let hi = this.heights.length // esclusivo, il caso limite è "nessuna"
    while (lo < hi) {
      const mid = (lo + hi) >>> 1
      if (this.prefix[mid]! < y) lo = mid + 1
      else hi = mid
    }
    return lo
  }

  /**
   * Righe da montare per `scrollTop`/`viewportHeight` correnti, con
   * `overscan` pixel di margine sopra e sotto — nel documento funzionale
   * (§66.3) "circa uno schermo e un quarto", perché lo scorrimento veloce
   * non arrivi mai prima del contenuto.
   */
  visibleRange(scrollTop: number, viewportHeight: number, overscan = 0): VisibleRange {
    if (this.heights.length === 0) return { start: 0, end: 0 }
    const from = Math.max(0, scrollTop - overscan)
    const to = scrollTop + viewportHeight + overscan
    const start = this.rowAtOffset(from)
    const end = this.firstRowStartingAtOrAfter(to)
    return { start, end }
  }
}

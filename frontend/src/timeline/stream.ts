import { justify } from './justify'
import type { TimelineGeometry } from './geometry'

/**
 * Costanti verificate alla fonte in `docs/ui/keeppix-mockup.html`
 * (`planPhotoRows`/`planStream`, righe 4593-4666), non stimate: il
 * prototipo calcola già questa stessa geometria per il proprio scrubber
 * dei mesi, ed è la fonte di verità per questi numeri.
 */
export const GRID_GAP = 6
export const MONTH_HEAD_H = 29 // .month-head (riga a 16px, ~19px) + margin-bottom 10px
export const MONTH_GAP = 26 // .month-block margin-bottom
export const STREAM_OVERSCAN = 1.25 // schermi di margine tenuti montati sopra e sotto

/** `targetH` del prototipo: `max(64, (larghezza - gap fra colonne) / colonne / 1.3)`. */
export function targetRowHeight(width: number, density: number): number {
  return Math.max(64, (width - (density - 1) * GRID_GAP) / density / 1.3)
}

export interface HeaderRow {
  type: 'header'
  /** `"YYYY-MM"`, lo stesso formato di `MonthBucket.month`. */
  month: string
  count: number
  height: number
}

export interface GridCell {
  x: number
  w: number
  h: number
  /** Posizione dello scatto dentro il mese, nello stesso ordine con cui
   * `/timeline?bucket=` restituisce le pagine di quel mese (`ORDER BY
   * taken_at_utc DESC, id DESC` sia lato geometria sia lato pagine) —
   * l'indice per risalire dall'oggetto geometria all'asset vero una
   * volta che la pagina di quel mese è caricata. */
  offsetInMonth: number
}

export interface GridRow {
  type: 'grid'
  month: string
  cells: GridCell[]
  height: number
}

export type StreamRow = HeaderRow | GridRow

export interface StreamPlan {
  rows: StreamRow[]
  /** Un'altezza di "slot" per riga: uguale a `rows[i].height`, salvo
   * sull'ultima riga di ogni mese dove include anche `MONTH_GAP` — lo
   * spazio vuoto verso il mese successivo è margine fra le sezioni, non
   * contenuto della riga stessa, ma va comunque contato perché il
   * virtualizzatore sappia dove comincia la riga dopo. */
  rowHeights: number[]
  totalHeight: number
}

/** `"YYYY-MM"` → `anno*12 + mese_di_calendario (1..=12)`, lo stesso indice
 * di `month_index` in `crates/keeppix-api/src/routes/timeline.rs`. */
export function monthIndexOf(month: string): number {
  const [year, mm] = month.split('-').map(Number)
  return year * 12 + mm
}

/** L'inverso di `monthIndexOf` — solo per il caso difensivo in cui la
 * geometria contenga un mese assente dall'elenco dei bucket (le due
 * richieste non condividono un'istantanea atomica del database). */
function monthLabelOf(index: number): string {
  const year = Math.floor((index - 1) / 12)
  const mm = index - year * 12
  return `${year}-${String(mm).padStart(2, '0')}`
}

/**
 * Costruisce il piano completo della timeline da geometria + conteggi dei
 * bucket: un'intestazione più N righe giustificate per ogni mese
 * effettivamente presente in `geometry`. Pura aritmetica sui rapporti
 * d'aspetto — nessun accesso al DOM — eseguibile su tutta la libreria in
 * un colpo solo (§66.3 del documento funzionale, e il commento del
 * prototipo su `planPhotoRows`: "si può eseguire su tutta la libreria in
 * un colpo solo").
 *
 * Guidato dai confini di mese della geometria stessa, non dall'elenco dei
 * bucket: quest'ultimo alimenta solo l'etichetta del conteggio mostrato
 * (`count`), con un ripiego sulla lunghezza del segmento se un mese della
 * geometria non ha un bucket corrispondente.
 */
export function planStream(
  geometry: TimelineGeometry,
  buckets: { month: string; count: number }[],
  width: number,
  density: number
): StreamPlan {
  const rows: StreamRow[] = []
  const rowHeights: number[] = []
  if (width <= 0 || geometry.count === 0) {
    return { rows, rowHeights, totalHeight: 0 }
  }

  const countByMonthIndex = new Map(buckets.map((b) => [monthIndexOf(b.month), b]))
  const targetH = targetRowHeight(width, density)
  let index = 0

  while (index < geometry.count) {
    const monthIndex = geometry.month(index)
    const items: { id: string; width: number; height: number }[] = []
    let offsetInMonth = 0
    while (index < geometry.count && geometry.month(index) === monthIndex) {
      items.push({
        id: String(offsetInMonth),
        width: geometry.width(index) || 1,
        height: geometry.height(index) || 1
      })
      offsetInMonth++
      index++
    }

    const known = countByMonthIndex.get(monthIndex)
    const month = known?.month ?? monthLabelOf(monthIndex)
    const count = known?.count ?? items.length

    rows.push({ type: 'header', month, count, height: MONTH_HEAD_H })
    rowHeights.push(MONTH_HEAD_H)

    for (const row of justify(items, width, targetH)) {
      rows.push({
        type: 'grid',
        month,
        height: row.height,
        cells: row.cells.map((cell) => ({ x: cell.x, w: cell.w, h: cell.h, offsetInMonth: Number(cell.id) }))
      })
      rowHeights.push(row.height)
    }

    // MONTH_GAP verso il mese successivo, contato sull'ultima riga appena
    // emessa: non è una riga a sé, solo lo spazio prima della prossima.
    rowHeights[rowHeights.length - 1] += MONTH_GAP
  }
  // L'ultimo mese non ha un "prossimo": lo spazio aggiunto sopra va tolto.
  rowHeights[rowHeights.length - 1] -= MONTH_GAP

  const totalHeight = rowHeights.reduce((a, b) => a + b, 0)
  return { rows, rowHeights, totalHeight }
}

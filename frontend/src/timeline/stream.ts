import { justify } from './justify'
import type { TimelineGeometry } from './geometry'

/**
 * Constants verified against the source in `docs/ui/keeppix-mockup.html`
 * (`planPhotoRows`/`planStream`), not estimated: the prototype already
 * computes this same geometry for its own month scrubber, and it's the
 * source of truth for these numbers.
 */
export const GRID_GAP = 6
export const MONTH_HEAD_H = 29 // .month-head (16px line, ~19px) + 10px margin-bottom
export const MONTH_GAP = 26 // .month-block margin-bottom
export const STREAM_OVERSCAN = 1.25 // screens of margin kept mounted above and below

/** `targetH` from the prototype: `max(64, (width - gap between columns) / columns / 1.3)`. */
export function targetRowHeight(width: number, density: number): number {
  return Math.max(64, (width - (density - 1) * GRID_GAP) / density / 1.3)
}

export interface HeaderRow {
  type: 'header'
  /** `"YYYY-MM"`, the same format as `MonthBucket.month`. */
  month: string
  count: number
  height: number
}

export interface GridCell {
  x: number
  w: number
  h: number
  /** Position of the shot within the month, in the same order
   * `/timeline?bucket=` returns that month's pages (`ORDER BY
   * taken_at_utc DESC, id DESC` on both the geometry and page sides) —
   * the index used to map from the geometry object back to the real
   * asset once that month's page has loaded. */
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
  /** A "slot" height per row: equal to `rows[i].height`, except on the
   * last row of each month, where it also includes `MONTH_GAP` — the
   * empty space toward the next month is margin between sections, not
   * the row's own content, but it still needs to be counted so the
   * virtualizer knows where the next row starts. */
  rowHeights: number[]
  totalHeight: number
}

/** `"YYYY-MM"` → `year*12 + calendar_month (1..=12)`, the same index as
 * `month_index` in `crates/keeppix-api/src/routes/timeline.rs`. */
export function monthIndexOf(month: string): number {
  const [year, mm] = month.split('-').map(Number)
  return year * 12 + mm
}

/** The inverse of `monthIndexOf` — only for the defensive case where the
 * geometry contains a month absent from the bucket list (the two
 * requests don't share an atomic database snapshot). */
function monthLabelOf(index: number): string {
  const year = Math.floor((index - 1) / 12)
  const mm = index - year * 12
  return `${year}-${String(mm).padStart(2, '0')}`
}

/**
 * Builds the complete timeline plan from geometry + bucket counts: a
 * header plus N justified rows for every month actually present in
 * `geometry`. Pure arithmetic over aspect ratios — no DOM access — so it
 * can run over the entire library in one shot, same as the prototype's
 * `planPhotoRows`.
 *
 * Driven by the geometry's own month boundaries, not by the bucket list:
 * the bucket list only feeds the displayed count label (`count`),
 * falling back to the segment's length if a month in the geometry has no
 * matching bucket.
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

    // MONTH_GAP toward the next month, counted on the row just emitted:
    // not a row of its own, just the space before the next one.
    rowHeights[rowHeights.length - 1] += MONTH_GAP
  }
  // The last month has no "next": the space added above must be removed.
  rowHeights[rowHeights.length - 1] -= MONTH_GAP

  const totalHeight = rowHeights.reduce((a, b) => a + b, 0)
  return { rows, rowHeights, totalHeight }
}

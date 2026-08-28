/**
 * A hand-written virtualizer: prefix sums of row heights plus binary
 * search on `scrollTop`. No library — a measurement-based virtualizer
 * would solve a problem that doesn't exist here, because the geometry
 * payload gives the width and height of every shot *before* drawing: row
 * heights can be computed exactly ahead of time, not measured row by row
 * as they enter the viewport.
 *
 * Agnostic to what a "row" *is* (a grid of photos, a month header): it
 * only receives an array of pixel heights, in the same order the rows
 * are drawn from top to bottom.
 */

export interface VisibleRange {
  /** Index of the first row to mount (inclusive). */
  start: number
  /** Index past the last row to mount (exclusive, like `Array.slice`). */
  end: number
}

export class RowVirtualizer {
  /** Total content height: gives the scrollbar its true length from the
   * very first instant, without having to mount a single row. */
  readonly totalHeight: number

  private readonly heights: readonly number[]
  /** `prefix[i]` = sum of the heights of all rows before `i`.
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

  /** The `y` coordinate (in pixels, from the top of the content) where the row starts. */
  rowTop(index: number): number {
    return this.prefix[index] ?? this.totalHeight
  }

  rowHeight(index: number): number {
    return this.heights[index] ?? 0
  }

  /**
   * Index of the row whose band `[top, top+height)` contains `y` — or the
   * last row if `y` falls past the end of the content. Binary search over
   * the prefix sums, `O(log n)`: this is the core of `visibleRange`,
   * together with `firstRowStartingAtOrAfter` below.
   */
  private rowAtOffset(y: number): number {
    if (this.heights.length === 0) return 0
    let lo = 0
    let hi = this.heights.length // exclusive
    while (lo < hi) {
      const mid = (lo + hi) >>> 1
      if (this.prefix[mid + 1]! <= y) lo = mid + 1
      else hi = mid
    }
    return Math.min(lo, this.heights.length - 1)
  }

  /**
   * Index of the first row that starts at or after `y` — the exclusive
   * upper bound of the range of rows starting before `y`. This isn't the
   * same computation as `rowAtOffset`: on an exact boundary between two
   * rows (`y` exactly equal to a `rowTop`), `rowAtOffset(y)` returns the
   * row that *starts* there (half-open interval convention
   * `[top, bottom)`), but that row doesn't overlap `[…, y)` — using
   * `rowAtOffset(to) + 1` as the upper bound would include one extra row
   * right on that boundary.
   */
  private firstRowStartingAtOrAfter(y: number): number {
    let lo = 0
    let hi = this.heights.length // exclusive, the limiting case is "none"
    while (lo < hi) {
      const mid = (lo + hi) >>> 1
      if (this.prefix[mid]! < y) lo = mid + 1
      else hi = mid
    }
    return lo
  }

  /**
   * Rows to mount for the current `scrollTop`/`viewportHeight`, with
   * `overscan` pixels of margin above and below — roughly one and a
   * quarter screens, so fast scrolling never outpaces the content.
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

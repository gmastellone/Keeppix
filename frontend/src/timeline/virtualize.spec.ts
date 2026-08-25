import { describe, expect, it } from 'vitest'

import { RowVirtualizer } from './virtualize'

/** Riferimento a scansione lineare, per verificare la ricerca binaria contro un'implementazione ovvia. */
function linearVisibleRange(heights: number[], scrollTop: number, viewportHeight: number, overscan: number) {
  const from = Math.max(0, scrollTop - overscan)
  const to = scrollTop + viewportHeight + overscan
  let y = 0
  let start = heights.length
  let end = 0
  for (let i = 0; i < heights.length; i++) {
    const top = y
    const bottom = y + heights[i]
    if (bottom > from && top < to) {
      start = Math.min(start, i)
      end = Math.max(end, i + 1)
    }
    y = bottom
  }
  return heights.length === 0 || start > end ? { start: 0, end: 0 } : { start, end }
}

describe('RowVirtualizer', () => {
  it('reports the true total height immediately, before any row is "mounted"', () => {
    const v = new RowVirtualizer([100, 200, 50, 400])
    expect(v.totalHeight).toBe(750)
  })

  it('an empty geometry has zero height and an empty visible range', () => {
    const v = new RowVirtualizer([])
    expect(v.totalHeight).toBe(0)
    expect(v.visibleRange(0, 800, 200)).toEqual({ start: 0, end: 0 })
  })

  it('rowTop is the cumulative sum of every prior row', () => {
    const v = new RowVirtualizer([100, 200, 50, 400])
    expect(v.rowTop(0)).toBe(0)
    expect(v.rowTop(1)).toBe(100)
    expect(v.rowTop(2)).toBe(300)
    expect(v.rowTop(3)).toBe(350)
  })

  it('visibleRange at the top includes only rows within the viewport plus overscan', () => {
    const v = new RowVirtualizer([100, 100, 100, 100, 100, 100, 100, 100, 100, 100])
    // viewport 250px tall at scrollTop 0, no overscan: rows 0,1,2 (0-100,100-200,200-300 overlaps 0-250)
    expect(v.visibleRange(0, 250, 0)).toEqual({ start: 0, end: 3 })
  })

  it('visibleRange scrolled into the middle matches a linear scan', () => {
    const heights = [80, 120, 60, 200, 90, 140, 70, 300, 110, 60]
    for (const scrollTop of [0, 50, 130, 300, 500, 900, 1000]) {
      expect(new RowVirtualizer(heights).visibleRange(scrollTop, 400, 150)).toEqual(
        linearVisibleRange(heights, scrollTop, 400, 150)
      )
    }
  })

  it('overscan extends the range on both edges without exceeding the row count', () => {
    const v = new RowVirtualizer([100, 100, 100, 100, 100])
    // scrollTop 200 (row 2), viewport 100, overscan 250 pulls in rows before and past the end
    expect(v.visibleRange(200, 100, 250)).toEqual({ start: 0, end: 5 })
  })

  it('scrolled past the end still returns the last row, not an out-of-range index', () => {
    const v = new RowVirtualizer([100, 100, 100])
    expect(v.visibleRange(10_000, 200, 0)).toEqual({ start: 2, end: 3 })
  })

  /**
   * L'assunzione da verificare del piano (Task 4): su una geometria da
   * 200.000 record, il numero di righe montate resta sotto una soglia
   * esplicita durante uno scroll simulato — non l'intera libreria in una
   * volta. `rowHeight` variabile (non costante) per non testare un caso
   * fortunato dove ogni riga ha la stessa altezza.
   */
  it('stays within an explicit mounted-row cap while scrolling a 200,000-row geometry', () => {
    const rowCount = 200_000
    const heights = Array.from({ length: rowCount }, (_, i) => 60 + (i % 5) * 30) // 60..180px
    const v = new RowVirtualizer(heights)
    expect(v.rowCount).toBe(rowCount)

    const viewportHeight = 900
    const overscan = viewportHeight * 1.25 // "circa uno schermo e un quarto", documento funzionale §66.3
    const maxMountedRows = 40 // ben sotto le ~3 schermate di tessere della spec §3, in righe

    const positions = [0, v.totalHeight * 0.1, v.totalHeight * 0.5, v.totalHeight * 0.9, v.totalHeight]
    for (const scrollTop of positions) {
      const { start, end } = v.visibleRange(scrollTop, viewportHeight, overscan)
      expect(end - start).toBeLessThanOrEqual(maxMountedRows)
      expect(start).toBeGreaterThanOrEqual(0)
      expect(end).toBeLessThanOrEqual(rowCount)
    }
  })
})

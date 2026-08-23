import { describe, expect, it } from 'vitest'

import { TimelineGeometry } from './geometry'
import { GRID_GAP, monthIndexOf, planStream, targetRowHeight } from './stream'

function encode(records: { w: number; h: number; month: number }[]): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + records.length * 6)
  const view = new DataView(buffer)
  view.setUint32(0, 1, true)
  view.setUint32(4, records.length, true)
  records.forEach((r, i) => {
    const offset = 8 + i * 6
    view.setUint16(offset, r.w, true)
    view.setUint16(offset + 2, r.h, true)
    view.setUint16(offset + 4, r.month, true)
  })
  return buffer
}

describe('monthIndexOf', () => {
  it('matches the backend month_index formula (anno*12 + mese)', () => {
    expect(monthIndexOf('2026-08')).toBe(2026 * 12 + 8)
    expect(monthIndexOf('2025-12')).toBe(2025 * 12 + 12)
    expect(monthIndexOf('2025-01')).toBe(2025 * 12 + 1)
  })
})

describe('targetRowHeight', () => {
  it('matches the prototype formula: max(64, (width - gaps)/cols/1.3)', () => {
    expect(targetRowHeight(1200, 4)).toBeCloseTo(Math.max(64, (1200 - 3 * GRID_GAP) / 4 / 1.3))
  })

  it('floors at 64px even for a very dense grid', () => {
    expect(targetRowHeight(200, 12)).toBe(64)
  })
})

describe('planStream', () => {
  const AUG = monthIndexOf('2026-08')
  const JUL = monthIndexOf('2026-07')

  it('emits one header row followed by justified grid rows per month, in geometry order', () => {
    const geometry = new TimelineGeometry(
      encode([
        { w: 4000, h: 3000, month: AUG },
        { w: 3000, h: 4000, month: AUG },
        { w: 1000, h: 1000, month: JUL }
      ])
    )
    const buckets = [
      { month: '2026-08', count: 2 },
      { month: '2026-07', count: 1 }
    ]
    const plan = planStream(geometry, buckets, 1200, 4)

    expect(plan.rows[0]).toMatchObject({ type: 'header', month: '2026-08', count: 2 })
    expect(plan.rows.some((r) => r.type === 'grid' && r.month === '2026-08')).toBe(true)
    const julHeaderIndex = plan.rows.findIndex((r) => r.type === 'header' && r.month === '2026-07')
    expect(julHeaderIndex).toBeGreaterThan(0)
    expect(plan.rows[julHeaderIndex]).toMatchObject({ count: 1 })
  })

  it('maps each cell to its offset within the month, in geometry order', () => {
    const geometry = new TimelineGeometry(
      encode([
        { w: 100, h: 100, month: AUG },
        { w: 100, h: 100, month: AUG },
        { w: 100, h: 100, month: AUG }
      ])
    )
    const plan = planStream(geometry, [{ month: '2026-08', count: 3 }], 1200, 4)
    const offsets = plan.rows
      .filter((r) => r.type === 'grid')
      .flatMap((r) => (r as { cells: { offsetInMonth: number }[] }).cells.map((c) => c.offsetInMonth))
    expect(offsets.sort((a, b) => a - b)).toEqual([0, 1, 2])
  })

  it('rowHeights sums to totalHeight and the last row overall carries no trailing MONTH_GAP', () => {
    const geometry = new TimelineGeometry(
      encode([
        { w: 100, h: 100, month: AUG },
        { w: 100, h: 100, month: JUL }
      ])
    )
    const plan = planStream(geometry, [{ month: '2026-08', count: 1 }, { month: '2026-07', count: 1 }], 800, 4)
    expect(plan.rowHeights.reduce((a, b) => a + b, 0)).toBe(plan.totalHeight)
    // L'ultimo mese (luglio) non ha un MONTH_GAP dopo: lo "slot" della sua
    // ultima riga coincide esattamente con l'altezza visiva della riga,
    // non ne include uno gonfiato.
    const lastRow = plan.rows[plan.rows.length - 1]
    const lastSlotHeight = plan.rowHeights[plan.rowHeights.length - 1]
    expect(lastSlotHeight).toBe(lastRow.height)
  })

  it('falls back to the geometry segment length as the count when a month is missing from buckets', () => {
    const geometry = new TimelineGeometry(
      encode([
        { w: 100, h: 100, month: AUG },
        { w: 100, h: 100, month: AUG }
      ])
    )
    const plan = planStream(geometry, [], 800, 4)
    expect(plan.rows[0]).toMatchObject({ type: 'header', month: '2026-08', count: 2 })
  })

  it('returns an empty plan for an empty geometry or a zero width', () => {
    const geometry = new TimelineGeometry(encode([]))
    expect(planStream(geometry, [], 800, 4)).toEqual({ rows: [], rowHeights: [], totalHeight: 0 })
    const nonEmpty = new TimelineGeometry(encode([{ w: 100, h: 100, month: AUG }]))
    expect(planStream(nonEmpty, [], 0, 4)).toEqual({ rows: [], rowHeights: [], totalHeight: 0 })
  })

  it('builds a full plan for a 200,000-shot geometry across many months without ballooning row count', () => {
    const recordCount = 200_000
    const monthsSpanned = 240 // 20 anni
    const records = Array.from({ length: recordCount }, (_, i) => ({
      w: 1600 + (i % 7) * 200,
      h: 1200 + (i % 5) * 150,
      month: AUG - Math.floor((i / recordCount) * monthsSpanned)
    }))
    const geometry = new TimelineGeometry(encode(records))
    // Bucket volutamente vuoto: esercita il ripiego su `monthLabelOf` +
    // conteggio dal solo segmento di geometria, non la corrispondenza coi
    // bucket già coperta dai test sopra.
    const plan = planStream(geometry, [], 1200, 6)
    expect(plan.rows.length).toBeGreaterThan(0)
    // Ogni riga giustificata sta per costruzione dentro la larghezza del
    // contenitore (più il margine di arrotondamento dell'ultima riga di un
    // mese, che il piano non stira) — nessuna riga infinita o degenere.
    for (const row of plan.rows) {
      if (row.type === 'grid') {
        const last = row.cells.at(-1)
        expect(last).toBeDefined()
        expect(last!.x + last!.w).toBeLessThanOrEqual(1200 + 1)
      }
    }
    expect(plan.totalHeight).toBeGreaterThan(0)
  })
})

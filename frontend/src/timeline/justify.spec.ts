import { describe, expect, it } from 'vitest'

import { clampDensity, justify } from './justify'

describe('justify', () => {
  it('keeps a panorama wider than a portrait in the same row', () => {
    const rows = justify(
      [
        { id: 'pano', width: 4000, height: 1000 },
        { id: 'port', width: 1000, height: 4000 }
      ],
      1200,
      200
    )
    expect(rows).toHaveLength(1)
    const [pano, port] = rows[0].cells
    expect(pano.w).toBeGreaterThan(port.w)
    expect(pano.h).toBe(port.h)
  })

  it('fills the container width on a complete row', () => {
    const rows = justify(
      [
        { id: 'a', width: 100, height: 100 },
        { id: 'b', width: 100, height: 100 },
        { id: 'c', width: 100, height: 100 },
        { id: 'd', width: 100, height: 100 }
      ],
      400,
      100
    )
    const last = rows[0].cells.at(-1)!
    expect(last.x + last.w).toBeCloseTo(400, 0)
  })
})

describe('clampDensity', () => {
  it('stays inside 2–12 on desktop (default)', () => {
    expect(clampDensity(1)).toBe(2)
    expect(clampDensity(12.4)).toBe(12)
    expect(clampDensity(7.2)).toBe(7)
  })

  it('stays inside 2–6 on mobile', () => {
    expect(clampDensity(1, true)).toBe(2)
    expect(clampDensity(9, true)).toBe(6)
    expect(clampDensity(4.4, true)).toBe(4)
  })
})

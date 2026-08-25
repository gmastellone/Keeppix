import { describe, expect, it } from 'vitest'

import { TimelineGeometry, UnsupportedGeometryFormatError } from './geometry'

/**
 * Stesso layout di `encode_geometry` in
 * `crates/keeppix-api/src/routes/timeline.rs`: intestazione (versione u32,
 * conteggio u32) + N record da 6 byte (w:u16, h:u16, month:u16), LE.
 */
function encode(records: { w: number; h: number; month: number }[], version = 1): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + records.length * 6)
  const view = new DataView(buffer)
  view.setUint32(0, version, true)
  view.setUint32(4, records.length, true)
  records.forEach((r, i) => {
    const offset = 8 + i * 6
    view.setUint16(offset, r.w, true)
    view.setUint16(offset + 2, r.h, true)
    view.setUint16(offset + 4, r.month, true)
  })
  return buffer
}

describe('TimelineGeometry', () => {
  it('decodes count and per-record width/height/month from the real byte layout', () => {
    const geometry = new TimelineGeometry(
      encode([
        { w: 4000, h: 3000, month: 24313 }, // 2026*12 + 1
        { w: 1080, h: 1920, month: 24312 }, // 2025*12 + 12
        { w: 0, h: 0, month: 24312 } // sizing non ancora arrivato (§ saturating_u16)
      ])
    )

    expect(geometry.count).toBe(3)
    expect(geometry.width(0)).toBe(4000)
    expect(geometry.height(0)).toBe(3000)
    expect(geometry.month(0)).toBe(24313)
    expect(geometry.width(1)).toBe(1080)
    expect(geometry.height(1)).toBe(1920)
    expect(geometry.month(1)).toBe(24312)
    expect(geometry.width(2)).toBe(0)
    expect(geometry.height(2)).toBe(0)
  })

  it('decodes an empty geometry (count 0, no records)', () => {
    const geometry = new TimelineGeometry(encode([]))
    expect(geometry.count).toBe(0)
  })

  it('rejects a format version it does not understand instead of reading garbage bytes', () => {
    const buffer = encode([{ w: 1, h: 1, month: 1 }], 2)
    expect(() => new TimelineGeometry(buffer)).toThrow(UnsupportedGeometryFormatError)
  })

  it('round-trips u16-max values (saturating_u16 ceiling on the backend)', () => {
    const geometry = new TimelineGeometry(encode([{ w: 65535, h: 65535, month: 65535 }]))
    expect(geometry.width(0)).toBe(65535)
    expect(geometry.height(0)).toBe(65535)
    expect(geometry.month(0)).toBe(65535)
  })

  describe('concat', () => {
    it('merges page buffers into one geometry with a summed count and records in order', () => {
      const page1 = encode([
        { w: 100, h: 200, month: 1 },
        { w: 101, h: 201, month: 1 }
      ])
      const page2 = encode([{ w: 102, h: 202, month: 2 }])
      const page3 = encode([
        { w: 103, h: 203, month: 3 },
        { w: 104, h: 204, month: 3 }
      ])

      const merged = TimelineGeometry.concat([page1, page2, page3])

      expect(merged.count).toBe(5)
      for (const [i, expected] of [
        [100, 200, 1],
        [101, 201, 1],
        [102, 202, 2],
        [103, 203, 3],
        [104, 204, 3]
      ].entries()) {
        expect(merged.width(i)).toBe(expected[0])
        expect(merged.height(i)).toBe(expected[1])
        expect(merged.month(i)).toBe(expected[2])
      }
    })

    it('returns the single buffer unchanged (as a real TimelineGeometry) when there is only one page', () => {
      const only = encode([{ w: 1, h: 2, month: 3 }])
      const merged = TimelineGeometry.concat([only])
      expect(merged.count).toBe(1)
      expect(merged.width(0)).toBe(1)
    })

    it('handles an empty page in the middle without corrupting the records after it', () => {
      const page1 = encode([{ w: 1, h: 1, month: 1 }])
      const empty = encode([])
      const page3 = encode([{ w: 2, h: 2, month: 2 }])
      const merged = TimelineGeometry.concat([page1, empty, page3])
      expect(merged.count).toBe(2)
      expect(merged.width(0)).toBe(1)
      expect(merged.width(1)).toBe(2)
    })
  })
})

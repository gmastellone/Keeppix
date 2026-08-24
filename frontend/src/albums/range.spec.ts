import { describe, expect, it } from 'vitest'

import { albumMonthRange } from './range'

describe('albumMonthRange', () => {
  it('returns null for a member list with no dated photos', () => {
    expect(albumMonthRange([], 'it')).toBeNull()
    expect(albumMonthRange([{ taken_at_utc: null }], 'it')).toBeNull()
  })

  it('collapses to a single month when every photo falls in the same month', () => {
    expect(
      albumMonthRange(
        [{ taken_at_utc: '2026-07-02T10:00:00Z' }, { taken_at_utc: '2026-07-28T10:00:00Z' }],
        'it'
      )
    ).toBe('luglio 2026')
  })

  it('spans first to last month, ignoring undated photos', () => {
    expect(
      albumMonthRange(
        [
          { taken_at_utc: '2026-03-15T10:00:00Z' },
          { taken_at_utc: null },
          { taken_at_utc: '2026-07-01T10:00:00Z' }
        ],
        'it'
      )
    ).toBe('marzo 2026 – luglio 2026')
  })
})

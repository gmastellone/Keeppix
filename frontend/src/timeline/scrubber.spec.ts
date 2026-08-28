import { describe, expect, it } from 'vitest'

import { monthAbbrev, monthAtOffset, monthFull } from './scrubber'

describe('monthAtOffset', () => {
  // Deliberately unbalanced counts: if the algorithm weighted by `count`
  // (an earlier, incorrect behavior — see the comment in the source),
  // the month with 90 shots would take up nearly the whole bar. Months
  // are equidistant instead.
  const buckets = [
    { month: '2024-08', count: 10 },
    { month: '2024-07', count: 90 },
    { month: '2024-06', count: 10 }
  ]

  it('maps the top of the track to the newest month', () => {
    expect(monthAtOffset(buckets, 0, 100)).toBe('2024-08')
  })

  it('maps the bottom of the track to the oldest month', () => {
    expect(monthAtOffset(buckets, 100, 100)).toBe('2024-06')
  })

  it('is equidistant by index, not weighted by count', () => {
    // 3 months, indices 0/1/2 over ratio 0..1: the exact center (ratio
    // 0.5) rounds to round(0.5*2)=1, the middle month — not the one with
    // the most shots, which isn't even in the middle of the list here.
    expect(monthAtOffset(buckets, 50, 100)).toBe('2024-07')
  })

  it('clamps an offset outside the track instead of returning nothing', () => {
    expect(monthAtOffset(buckets, -20, 100)).toBe('2024-08')
    expect(monthAtOffset(buckets, 500, 100)).toBe('2024-06')
  })

  it('returns undefined for an empty bucket list or a zero-height track', () => {
    expect(monthAtOffset([], 0, 100)).toBeUndefined()
    expect(monthAtOffset(buckets, 0, 0)).toBeUndefined()
  })
})

describe('monthAbbrev', () => {
  it('formats via Intl, localized — not a hardcoded Italian table', () => {
    expect(monthAbbrev('2026-07', 'en')).toBe('Jul')
    expect(monthAbbrev('2026-07', 'it')).toBe('lug')
  })
})

describe('monthFull', () => {
  it('formats the full month name plus year, localized', () => {
    expect(monthFull('2026-07', 'en')).toBe('July 2026')
    expect(monthFull('2026-07', 'it')).toBe('luglio 2026')
  })

  it('does not drift across a month boundary regardless of the runtime timezone', () => {
    // A month built from day 1 in UTC: if the formatting didn't force
    // timeZone:'UTC', a negative offset timezone could read the 30th of
    // the previous month.
    expect(monthFull('2026-01', 'en')).toBe('January 2026')
    expect(monthFull('2025-12', 'en')).toBe('December 2025')
  })
})

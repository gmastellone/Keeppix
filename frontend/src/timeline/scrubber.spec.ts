import { describe, expect, it } from 'vitest'

import { monthAtOffset } from './scrubber'

describe('monthAtOffset', () => {
  const buckets = [
    { month: '2024-08', count: 10 },
    { month: '2024-07', count: 90 }
  ]

  it('maps the top of the track to the newest month', () => {
    expect(monthAtOffset(buckets, 0, 100)).toBe('2024-08')
  })

  it('weights months by count so a large older month occupies more of the track', () => {
    expect(monthAtOffset(buckets, 20, 100)).toBe('2024-07')
  })
})

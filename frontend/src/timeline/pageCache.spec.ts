import { describe, expect, it } from 'vitest'

import { LruPageCache } from './pageCache'

describe('LruPageCache', () => {
  it('stores and returns a value under its capacity', () => {
    const cache = new LruPageCache<string, number[]>(3)
    cache.set('2026-08', [1, 2, 3])
    expect(cache.get('2026-08')).toEqual([1, 2, 3])
    expect(cache.has('2026-08')).toBe(true)
    expect(cache.size).toBe(1)
  })

  it('a missing key returns undefined', () => {
    const cache = new LruPageCache<string, number[]>(3)
    expect(cache.get('missing')).toBeUndefined()
    expect(cache.has('missing')).toBe(false)
  })

  it('evicts the least-recently-used entry once capacity is exceeded', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.set('c', 3) // 'a' is oldest untouched, gets evicted

    expect(cache.has('a')).toBe(false)
    expect(cache.has('b')).toBe(true)
    expect(cache.has('c')).toBe(true)
    expect(cache.size).toBe(2)
  })

  it('a get() refreshes recency, so a recently-read entry survives eviction', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.get('a') // touch 'a' — now 'b' is the least-recently-used
    cache.set('c', 3)

    expect(cache.has('a')).toBe(true)
    expect(cache.has('b')).toBe(false)
    expect(cache.has('c')).toBe(true)
  })

  it('re-setting an existing key refreshes its recency without growing size', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.set('a', 10) // 'a' re-set — now 'b' is the least-recently-used
    cache.set('c', 3)

    expect(cache.size).toBe(2)
    expect(cache.get('a')).toBe(10)
    expect(cache.has('b')).toBe(false)
  })

  it('delete() removes an entry outright', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.delete('a')
    expect(cache.has('a')).toBe(false)
    expect(cache.size).toBe(0)
  })

  it('clear() empties the cache outright', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.clear()
    expect(cache.size).toBe(0)
    expect(cache.has('a')).toBe(false)
    expect(cache.has('b')).toBe(false)
  })

  it('entries() iterates every resident page without disturbing recency', () => {
    const cache = new LruPageCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)

    expect(new Map(cache.entries())).toEqual(
      new Map([
        ['a', 1],
        ['b', 2]
      ])
    )

    // Reading via entries() must not count as a "use": 'a' is still the
    // least-recently-used and still the one evicted next.
    cache.set('c', 3)
    expect(cache.has('a')).toBe(false)
    expect(cache.has('b')).toBe(true)
  })

  it('rejects a capacity below 1', () => {
    expect(() => new LruPageCache(0)).toThrow(RangeError)
  })

  /**
   * Verifies that after a fully simulated scroll (here: 4000 months
   * loaded in sequence, well beyond the 214,000 real shots of the
   * reference library) the cache never exceeds its declared cap — the
   * evicted pages, not the new ones, pay the cost.
   */
  it('never exceeds its cap while simulating a full scroll through a very large library', () => {
    const capacity = 50
    const cache = new LruPageCache<number, string[]>(capacity)
    for (let month = 0; month < 4000; month++) {
      cache.set(month, [`asset-${month}-a`, `asset-${month}-b`])
      expect(cache.size).toBeLessThanOrEqual(capacity)
    }
    expect(cache.size).toBe(capacity)
    // Only the last `capacity` months seen stay resident.
    for (let month = 4000 - capacity; month < 4000; month++) {
      expect(cache.has(month)).toBe(true)
    }
    expect(cache.has(4000 - capacity - 1)).toBe(false)
  })
})

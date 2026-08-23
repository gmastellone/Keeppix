import { describe, expect, it } from 'vitest'

import { activeFilterCount, type MatchDimension, matchesFilters } from './quickFilter'

interface Photo {
  id: string
  type: 'raw' | 'jpeg'
  camera: string
  tags: string[]
}

const PHOTOS: Photo[] = [
  { id: 'a', type: 'raw', camera: 'X-T5', tags: ['sunset', 'mountain'] },
  { id: 'b', type: 'jpeg', camera: 'X-T5', tags: ['portrait'] },
  { id: 'c', type: 'raw', camera: 'A7IV', tags: ['sunset'] }
]

const DIMENSIONS: MatchDimension<Photo>[] = [
  { id: 'type', getValues: (p) => [p.type] },
  { id: 'camera', getValues: (p) => [p.camera] },
  { id: 'tags', getValues: (p) => p.tags }
]

describe('activeFilterCount', () => {
  it('sums the chosen values across all dimensions, not the number of active dimensions', () => {
    const selection = { type: new Set(['raw']), tags: new Set(['sunset', 'mountain']) }

    expect(activeFilterCount(selection)).toBe(3)
  })

  it('is zero when nothing is selected anywhere', () => {
    expect(activeFilterCount({})).toBe(0)
  })
})

describe('matchesFilters', () => {
  it('passes everything when no dimension is active', () => {
    expect(matchesFilters(PHOTOS[0], DIMENSIONS, {})).toBe(true)
  })

  it('ORs within one dimension: matches if any chosen value is present', () => {
    const selection = { tags: new Set(['portrait', 'mountain']) }

    expect(matchesFilters(PHOTOS[0], DIMENSIONS, selection)).toBe(true) // has "mountain"
    expect(matchesFilters(PHOTOS[2], DIMENSIONS, selection)).toBe(false) // has neither
  })

  it('ANDs across dimensions: must pass every active one', () => {
    const selection = { type: new Set(['raw']), camera: new Set(['A7IV']) }

    expect(matchesFilters(PHOTOS[0], DIMENSIONS, selection)).toBe(false) // raw, but wrong camera
    expect(matchesFilters(PHOTOS[2], DIMENSIONS, selection)).toBe(true) // raw and A7IV
  })

  it('a dimension whose values are always empty acts as a hard false when active — same as "people disabled"', () => {
    const disabledPeople: MatchDimension<Photo> = { id: 'people', getValues: () => [] }
    const selection = { people: new Set(['marta']) }

    expect(matchesFilters(PHOTOS[0], [...DIMENSIONS, disabledPeople], selection)).toBe(false)
  })
})

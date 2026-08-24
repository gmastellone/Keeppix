import { describe, expect, it } from 'vitest'

import { albumCoverGradient } from './albumCover'

describe('albumCoverGradient', () => {
  it('is deterministic: the same seed always yields the same gradient', () => {
    expect(albumCoverGradient('album-1')).toBe(albumCoverGradient('album-1'))
  })

  it('produces a valid linear-gradient() with two hsl() stops', () => {
    const gradient = albumCoverGradient('Migliori scatti 2026')
    expect(gradient).toMatch(/^linear-gradient\(135deg, hsl\(\d+, 40%, 55%\), hsl\(\d+, 35%, 32%\)\)$/)
  })

  it('different seeds usually produce different gradients', () => {
    expect(albumCoverGradient('album-1')).not.toBe(albumCoverGradient('album-2'))
  })
})

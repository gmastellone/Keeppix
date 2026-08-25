import { describe, expect, it } from 'vitest'

import { avatarColorFor } from './avatarColor'

describe('avatarColorFor', () => {
  it('is deterministic: the same seed always yields the same color', () => {
    expect(avatarColorFor('user-1')).toBe(avatarColorFor('user-1'))
  })

  it('produces a valid hsl() string with a hue in [0, 360)', () => {
    const color = avatarColorFor('Elena Bianchi')
    const match = /^hsl\((\d+), 55%, 40%\)$/.exec(color)
    expect(match).not.toBeNull()
    const hue = Number(match?.[1])
    expect(hue).toBeGreaterThanOrEqual(0)
    expect(hue).toBeLessThan(360)
  })

  it('different seeds usually produce different colors', () => {
    expect(avatarColorFor('Mich')).not.toBe(avatarColorFor('Elena Bianchi'))
  })
})

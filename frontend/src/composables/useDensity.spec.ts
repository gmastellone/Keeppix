import { describe, expect, it, beforeEach } from 'vitest'

import { useDensity } from './useDensity'

beforeEach(() => {
  localStorage.clear()
})

describe('useDensity', () => {
  it('defaults to 6 when nothing is stored yet', () => {
    const { density } = useDensity()
    expect(density.value).toBe(6)
  })

  it('reads a previously stored value', () => {
    localStorage.setItem('keeppix.density', '9')
    const { density } = useDensity()
    expect(density.value).toBe(9)
  })

  it('clamps an out-of-range stored value', () => {
    localStorage.setItem('keeppix.density', '99')
    const { density } = useDensity()
    expect(density.value).toBe(12)
  })

  it('setDensity clamps and persists', () => {
    const { density, setDensity } = useDensity()
    setDensity(1)
    expect(density.value).toBe(2)
    expect(localStorage.getItem('keeppix.density')).toBe('2')
  })

  it('two independent calls share the same localStorage key — Timeline and Preferiti stay in sync', () => {
    const a = useDensity()
    a.setDensity(8)

    const b = useDensity()
    expect(b.density.value).toBe(8)
  })
})

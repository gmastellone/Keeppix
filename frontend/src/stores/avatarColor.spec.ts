import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { useAvatarColorStore } from './avatarColor'

beforeEach(() => {
  setActivePinia(createPinia())
  localStorage.clear()
})

afterEach(() => {
  localStorage.clear()
})

describe('useAvatarColorStore — §61.2 Colore avatar', () => {
  it('defaults to "accent" (Arancione) before load()', () => {
    const store = useAvatarColorStore()
    expect(store.colorId).toBe('accent')
    expect(store.hex).toBeNull()
  })

  it('load() reads a previously stored color for that user', () => {
    localStorage.setItem('keeppix.avatarColor.u1', 'viola')
    const store = useAvatarColorStore()

    store.load('u1')

    expect(store.colorId).toBe('viola')
    expect(store.hex).toBe('#8B5CF6')
  })

  it('setColor() applies and persists, keyed by user id', () => {
    const store = useAvatarColorStore()

    store.setColor('u1', 'rosso')

    expect(store.colorId).toBe('rosso')
    expect(store.hex).toBe('#D9503F')
    expect(localStorage.getItem('keeppix.avatarColor.u1')).toBe('rosso')
  })

  it('a color chosen by one user never leaks into another user sharing the same browser', () => {
    const store = useAvatarColorStore()
    store.setColor('u1', 'rosso')

    store.load('u2')

    expect(store.colorId).toBe('accent')
  })

  it('falls back to "accent" on a corrupted or unknown stored value', () => {
    localStorage.setItem('keeppix.avatarColor.u1', 'not-a-real-color')
    const store = useAvatarColorStore()

    store.load('u1')

    expect(store.colorId).toBe('accent')
  })

  it('reset() clears back to the default, for logout', () => {
    const store = useAvatarColorStore()
    store.setColor('u1', 'blu')

    store.reset()

    expect(store.colorId).toBe('accent')
  })
})

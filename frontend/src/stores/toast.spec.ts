import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { setLocale } from '@/i18n'

import { useToastStore } from './toast'

describe('toast store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
    setLocale('it')
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('becomes visible after the 10ms show delay', () => {
    const store = useToastStore()
    store.show('Fatto')
    expect(store.toasts[0]?.visible).toBe(false)

    vi.advanceTimersByTime(10)
    expect(store.toasts[0]?.visible).toBe(true)
  })

  it('an ok toast without an action lives 2400ms then is removed 250ms later', () => {
    const store = useToastStore()
    store.show('Fatto')
    vi.advanceTimersByTime(10)

    // The close timer is armed from show()'s call time (t=0), not from t=10
    // after the show delay — advance to t=2399, one ms before it fires.
    vi.advanceTimersByTime(2389)
    expect(store.toasts).toHaveLength(1)
    expect(store.toasts[0]?.visible).toBe(true)

    vi.advanceTimersByTime(1)
    expect(store.toasts[0]?.visible).toBe(false)
    expect(store.toasts).toHaveLength(1)

    vi.advanceTimersByTime(249)
    expect(store.toasts).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(store.toasts).toHaveLength(0)
  })

  it('an error toast without an action lives 4200ms, not 2400ms', () => {
    const store = useToastStore()
    store.showError('Errore di rete')
    vi.advanceTimersByTime(10)

    vi.advanceTimersByTime(2400)
    expect(store.toasts[0]?.visible).toBe(true)

    vi.advanceTimersByTime(1800)
    expect(store.toasts[0]?.visible).toBe(false)
  })

  it('a toast with an action lives 6500ms regardless of kind', () => {
    const store = useToastStore()
    const retry = vi.fn()
    store.showError('Errore di rete', retry)
    vi.advanceTimersByTime(10)

    vi.advanceTimersByTime(4200)
    expect(store.toasts[0]?.visible).toBe(true)

    // Close was armed from t=0 for 6500ms; we're at t=4210 — advance to
    // t=6499, one ms before it fires.
    vi.advanceTimersByTime(2289)
    expect(store.toasts[0]?.visible).toBe(true)
    vi.advanceTimersByTime(1)
    expect(store.toasts[0]?.visible).toBe(false)
  })

  it('pausing on hover stops the timer, resuming restarts the full life', () => {
    const store = useToastStore()
    const retry = vi.fn()
    store.showError('Errore di rete', retry)
    const id = store.toasts[0]!.id

    vi.advanceTimersByTime(6000)
    store.pause(id)
    vi.advanceTimersByTime(10_000)
    expect(store.toasts[0]?.visible).toBe(true)

    store.resume(id)
    vi.advanceTimersByTime(6499)
    expect(store.toasts[0]?.visible).toBe(true)
    vi.advanceTimersByTime(1)
    expect(store.toasts[0]?.visible).toBe(false)
  })

  it('runAction closes the toast and runs the retry callback', () => {
    const store = useToastStore()
    const retry = vi.fn()
    store.showError('Errore di rete', retry)
    const id = store.toasts[0]!.id

    store.runAction(id)
    expect(retry).toHaveBeenCalledOnce()
    expect(store.toasts[0]?.visible).toBe(false)
  })

  it('showPartial formats the exact italian singular/plural wording', () => {
    const store = useToastStore()
    store.showPartial(9, 1)
    expect(store.toasts[0]?.message).toBe('9 su 10 completate — 1 non è riuscita.')

    store.showPartial(3, 2)
    expect(store.toasts[1]?.message).toBe('3 su 5 completate — 2 non sono riuscite.')
  })

  it("showPartial's retry action carries the failed count in its label", () => {
    const store = useToastStore()
    store.showPartial(9, 1, vi.fn())
    expect(store.toasts[0]?.action?.label).toBe('Riprova le 1 rimaste')
  })
})

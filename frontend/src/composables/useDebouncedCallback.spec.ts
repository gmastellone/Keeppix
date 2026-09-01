import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useDebouncedCallback } from './useDebouncedCallback'

// `useDebouncedCallback` calls `onUnmounted`, so it needs a mounted host
// component to run in — same approach as `useDensity.spec.ts`.
function mountDebounced(fn: () => void, delayMs: number) {
  let schedule: (() => void) | undefined
  const Host = defineComponent({
    setup() {
      schedule = useDebouncedCallback(fn, delayMs)
      return {}
    },
    template: '<div />'
  })
  const wrapper = mount(Host)
  if (!schedule) throw new Error('useDebouncedCallback did not run')
  return { wrapper, schedule }
}

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useDebouncedCallback', () => {
  it('fires once after the delay, not on every call', () => {
    const fn = vi.fn()
    const { schedule } = mountDebounced(fn, 500)

    schedule()
    schedule()
    schedule()
    expect(fn).not.toHaveBeenCalled()

    vi.advanceTimersByTime(500)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  it('a burst of calls collapses to one firing after the last call goes quiet', () => {
    const fn = vi.fn()
    const { schedule } = mountDebounced(fn, 500)

    schedule()
    vi.advanceTimersByTime(300)
    schedule()
    vi.advanceTimersByTime(300)
    schedule()
    // Only 300ms since the last call each time: never quiet long enough.
    expect(fn).not.toHaveBeenCalled()

    vi.advanceTimersByTime(500)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  it('does not fire against an unmounted component', () => {
    const fn = vi.fn()
    const { schedule, wrapper } = mountDebounced(fn, 500)

    schedule()
    wrapper.unmount()
    vi.advanceTimersByTime(500)

    expect(fn).not.toHaveBeenCalled()
  })
})

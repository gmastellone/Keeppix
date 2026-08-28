import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useDensity } from './useDensity'

const fetchPreferencesMock = vi.fn()
const patchPreferencesMock = vi.fn()

vi.mock('@/api/preferences', () => ({
  fetchPreferences: (...args: unknown[]) => fetchPreferencesMock(...args),
  patchPreferences: (...args: unknown[]) => patchPreferencesMock(...args)
}))

// `useDensity` calls `onMounted`/`onBeforeUnmount`, so those hooks simply
// don't fire outside a mounted component (Vue only warns, it doesn't
// error). A small host component works around that, same approach as
// `AlbumPickerDialog.spec.ts`.
function mountDensity(isMobile: boolean) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches: isMobile,
      media: '',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn()
    })
  )

  let exposed: ReturnType<typeof useDensity> | undefined
  const Host = defineComponent({
    setup() {
      exposed = useDensity()
      return {}
    },
    template: '<div />'
  })
  const wrapper = mount(Host)
  if (!exposed) throw new Error('useDensity did not run')
  return { wrapper, ...exposed }
}

beforeEach(() => {
  fetchPreferencesMock.mockResolvedValue({
    theme: 'chiaro',
    grid_density: { desktop: 4, mobile: 3 },
    notifications: { digest: true, condivisioni: true, problemi: true },
    language: 'it'
  })
  patchPreferencesMock.mockResolvedValue(null)
})

afterEach(() => {
  vi.clearAllMocks()
  vi.unstubAllGlobals()
})

async function flush() {
  await Promise.resolve()
  await Promise.resolve()
}

describe('useDensity — grid density', () => {
  it('starts at the documented default (4 desktop) before the server value loads', () => {
    const { density } = mountDensity(false)
    expect(density.value).toBe(4)
  })

  it('starts at the documented default (3 mobile) before the server value loads', () => {
    const { density } = mountDensity(true)
    expect(density.value).toBe(3)
  })

  it('reconciles with the real desktop value once preferences load', async () => {
    fetchPreferencesMock.mockResolvedValue({
      theme: 'chiaro',
      grid_density: { desktop: 9, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const { density } = mountDensity(false)
    await flush()

    expect(density.value).toBe(9)
  })

  it('reconciles with the real mobile value once preferences load, clamped to 2-6', async () => {
    fetchPreferencesMock.mockResolvedValue({
      theme: 'chiaro',
      grid_density: { desktop: 4, mobile: 99 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const { density } = mountDensity(true)
    await flush()

    expect(density.value).toBe(6)
  })

  it('setDensity on desktop clamps to 2-12 and patches only grid_density.desktop', async () => {
    const { density, setDensity } = mountDensity(false)
    await flush()

    setDensity(20)

    expect(density.value).toBe(12)
    expect(patchPreferencesMock).toHaveBeenCalledWith({ grid_density: { desktop: 12 } })
  })

  it('setDensity on mobile clamps to 2-6 and patches only grid_density.mobile', async () => {
    const { density, setDensity } = mountDensity(true)
    await flush()

    setDensity(20)

    expect(density.value).toBe(6)
    expect(patchPreferencesMock).toHaveBeenCalledWith({ grid_density: { mobile: 6 } })
  })

  it('keeps the default when the preferences request fails', async () => {
    fetchPreferencesMock.mockRejectedValue(new Error('offline'))
    const { density } = mountDensity(false)
    await flush()

    expect(density.value).toBe(4)
  })
})

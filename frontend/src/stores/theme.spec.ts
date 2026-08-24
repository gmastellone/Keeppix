import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { fetchPreferences, patchPreferences } from '@/api/preferences'

import { useThemeStore } from './theme'

vi.mock('@/api/preferences', () => ({
  fetchPreferences: vi.fn(),
  patchPreferences: vi.fn()
}))

function stubSystemDark(matches: boolean) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches,
      media: '',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn()
    })
  )
}

beforeEach(() => {
  setActivePinia(createPinia())
  document.documentElement.removeAttribute('data-theme')
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  document.documentElement.removeAttribute('data-theme')
})

describe('useThemeStore — §60.1 Aspetto', () => {
  it('applies "light" for the documented default (chiaro) even before load()', () => {
    stubSystemDark(false)
    const theme = useThemeStore()
    expect(theme.preference).toBe('chiaro')
  })

  it('load() reads the real preference and writes data-theme on <html>', async () => {
    stubSystemDark(false)
    vi.mocked(fetchPreferences).mockResolvedValue({
      theme: 'scuro',
      grid_density: { desktop: 4, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const theme = useThemeStore()

    await theme.load()

    expect(theme.preference).toBe('scuro')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(theme.loaded).toBe(true)
  })

  it('"sistema" resolves via matchMedia at load time', async () => {
    stubSystemDark(true)
    vi.mocked(fetchPreferences).mockResolvedValue({
      theme: 'sistema',
      grid_density: { desktop: 4, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const theme = useThemeStore()

    await theme.load()

    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('"sistema" reacts live to a system preference change', async () => {
    // Stessa `MediaQueryList` (finta) per tutto il test: un browser vero
    // aggiorna `.matches` sull'oggetto esistente prima di sparare
    // `change`, non ne restituisce uno nuovo da una seconda chiamata a
    // `matchMedia` — mutare qui lo stesso oggetto imita quel comportamento.
    let onChange: (() => void) | undefined
    const query = {
      matches: false,
      media: '',
      addEventListener: (_event: string, cb: () => void) => {
        onChange = cb
      },
      removeEventListener: vi.fn()
    }
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue(query))
    vi.mocked(fetchPreferences).mockResolvedValue({
      theme: 'sistema',
      grid_density: { desktop: 4, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const theme = useThemeStore()
    await theme.load()
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')

    query.matches = true
    onChange?.()

    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('setPreference applies immediately and persists via PATCH', async () => {
    stubSystemDark(false)
    vi.mocked(patchPreferences).mockResolvedValue({
      theme: 'scuro',
      grid_density: { desktop: 4, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const theme = useThemeStore()

    await theme.setPreference('scuro')

    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(patchPreferences).toHaveBeenCalledWith({ theme: 'scuro' })
  })

  it('setPreference rolls back on a failed PATCH', async () => {
    stubSystemDark(false)
    vi.mocked(patchPreferences).mockRejectedValue(new Error('network'))
    const theme = useThemeStore()

    await expect(theme.setPreference('scuro')).rejects.toThrow()

    expect(theme.preference).toBe('chiaro')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
  })

  it('reset() clears the applied theme and preference, for logout', async () => {
    stubSystemDark(false)
    vi.mocked(fetchPreferences).mockResolvedValue({
      theme: 'scuro',
      grid_density: { desktop: 4, mobile: 3 },
      notifications: { digest: true, condivisioni: true, problemi: true },
      language: 'it'
    })
    const theme = useThemeStore()
    await theme.load()

    theme.reset()

    expect(theme.preference).toBe('chiaro')
    expect(theme.loaded).toBe(false)
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
  })

  it('keeps the "chiaro" default when the preferences request fails', async () => {
    stubSystemDark(false)
    vi.mocked(fetchPreferences).mockRejectedValue(new Error('offline'))
    const theme = useThemeStore()

    await theme.load()

    expect(theme.preference).toBe('chiaro')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
  })
})

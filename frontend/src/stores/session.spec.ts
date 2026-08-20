import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from '@/api/client'
import { SESSION_REFRESH_INTERVAL_MS, useSessionStore } from '@/stores/session'

vi.mock('@/api/auth', () => ({
  getSetupStatus: vi.fn(),
  me: vi.fn(),
  login: vi.fn(),
  logout: vi.fn(),
  setupAccount: vi.fn(),
  refresh: vi.fn()
}))

vi.mock('@/api/users', () => ({
  updateUser: vi.fn()
}))

const auth = await import('@/api/auth')

const user = {
  id: 'u1',
  username: 'giovanni',
  display_name: 'Giovanni',
  email: null,
  role: 'admin' as const,
  locale: null
}

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state
  })
}

describe('session bootstrap', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.resetAllMocks()
    vi.useRealTimers()
    setVisibility('visible')
  })

  afterEach(() => {
    useSessionStore().stopWatchdog()
    vi.useRealTimers()
  })

  it('su 503 marca unavailable e non lancia', async () => {
    vi.mocked(auth.getSetupStatus).mockRejectedValue(
      new ApiProblem('keeppix/service-unavailable', 'unavailable', 503)
    )
    const session = useSessionStore()
    await expect(session.bootstrap()).resolves.toBeUndefined()
    expect(session.unavailable).toBe(true)
    expect(session.ready).toBe(true)
    expect(session.user).toBeNull()
  })

  it('su 401 di me() è sessione assente, non un outage', async () => {
    vi.mocked(auth.getSetupStatus).mockResolvedValue({ initialised: true })
    vi.mocked(auth.me).mockRejectedValue(
      new ApiProblem('keeppix/unauthenticated', 'unauthenticated', 401)
    )
    const session = useSessionStore()
    await session.bootstrap()
    expect(session.unavailable).toBe(false)
    expect(session.user).toBeNull()
    expect(session.ready).toBe(true)
  })

  it('login applica users.locale al i18n', async () => {
    vi.mocked(auth.login).mockResolvedValue({
      user: { ...user, locale: 'it' }
    })
    const session = useSessionStore()
    await session.login('giovanni', 'correct horse battery staple')
    expect(session.user?.locale).toBe('it')
    const { i18n } = await import('@/i18n')
    expect(i18n.global.locale.value).toBe('it')
  })
})

describe('session refresh watchdog', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.resetAllMocks()
    vi.useFakeTimers()
    setVisibility('visible')
    vi.mocked(auth.refresh).mockResolvedValue(null)
    vi.mocked(auth.logout).mockResolvedValue(null)
  })

  afterEach(() => {
    useSessionStore().stopWatchdog()
    vi.useRealTimers()
  })

  async function loggedIn() {
    vi.mocked(auth.login).mockResolvedValue({ user })
    const session = useSessionStore()
    await session.login('giovanni', 'correct horse battery staple')
    return session
  }

  it('rinnova la sessione a intervallo mentre la scheda è visibile', async () => {
    await loggedIn()
    expect(auth.refresh).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(auth.refresh).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(auth.refresh).toHaveBeenCalledTimes(2)
  })

  it('non rinnova se la scheda è nascosta', async () => {
    setVisibility('hidden')
    await loggedIn()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS * 2)
    expect(auth.refresh).not.toHaveBeenCalled()
  })

  it('al ritorno in primo piano rinnova, senza aspettare l’intervallo', async () => {
    setVisibility('hidden')
    await loggedIn()
    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(auth.refresh).not.toHaveBeenCalled()

    setVisibility('visible')
    document.dispatchEvent(new Event('visibilitychange'))
    await vi.advanceTimersByTimeAsync(0)
    expect(auth.refresh).toHaveBeenCalledTimes(1)
  })

  it('un 401 del rinnovo azzera l’utente: il prossimo giro è il login', async () => {
    vi.mocked(auth.refresh).mockRejectedValue(
      new ApiProblem('keeppix/unauthenticated', 'unauthenticated', 401)
    )
    const session = await loggedIn()
    expect(session.user).not.toBeNull()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(session.user).toBeNull()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(auth.refresh).toHaveBeenCalledTimes(1)
  })

  it('un 503 del rinnovo non espelle: il database è giù, la sessione no', async () => {
    vi.mocked(auth.refresh).mockRejectedValue(
      new ApiProblem('keeppix/service-unavailable', 'unavailable', 503)
    )
    const session = await loggedIn()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS)
    expect(session.user).toEqual(user)
  })

  it('dopo il logout il watchdog non chiama più refresh', async () => {
    const session = await loggedIn()
    await session.logout()
    vi.mocked(auth.refresh).mockClear()

    await vi.advanceTimersByTimeAsync(SESSION_REFRESH_INTERVAL_MS * 2)
    expect(auth.refresh).not.toHaveBeenCalled()
  })
})

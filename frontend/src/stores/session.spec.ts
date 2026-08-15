import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'

vi.mock('@/api/auth', () => ({
  getSetupStatus: vi.fn(),
  me: vi.fn(),
  login: vi.fn(),
  logout: vi.fn(),
  setupAccount: vi.fn()
}))

const auth = await import('@/api/auth')

describe('session bootstrap', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.resetAllMocks()
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
})

import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'

vi.mock('@/api/auth', () => ({
  getSetupStatus: vi.fn(),
  me: vi.fn(),
  login: vi.fn(),
  logout: vi.fn(),
  setupAccount: vi.fn(),
  refresh: vi.fn()
}))

vi.mock('@/api/libraries', () => ({
  fetchLibraries: vi.fn()
}))

const auth = await import('@/api/auth')
const libraries = await import('@/api/libraries')
const { router } = await import('@/router')

const user = {
  id: 'u1',
  username: 'giovanni',
  display_name: 'Giovanni',
  email: null,
  role: 'admin' as const,
  locale: null
}

describe('router: setup wizard resume', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.resetAllMocks()
  })

  afterEach(() => {
    useSessionStore().stopWatchdog()
  })

  it("sends an admin'd session with no library to /setup instead of stranding it at /", async () => {
    vi.mocked(auth.getSetupStatus).mockResolvedValue({ initialised: true })
    vi.mocked(auth.me).mockResolvedValue({ user })
    vi.mocked(libraries.fetchLibraries).mockResolvedValue([])

    await router.push('/folders')
    expect(router.currentRoute.value.path).toBe('/setup')
  })

  it('lets a session with a library through to the page it asked for', async () => {
    vi.mocked(auth.getSetupStatus).mockResolvedValue({ initialised: true })
    vi.mocked(auth.me).mockResolvedValue({ user })
    vi.mocked(libraries.fetchLibraries).mockResolvedValue([{ id: 'l1' }] as Awaited<
      ReturnType<typeof libraries.fetchLibraries>
    >)

    await router.push('/folders')
    expect(router.currentRoute.value.path).toBe('/folders')
  })

  it('leaves an unauthenticated visitor on /login, not /setup', async () => {
    vi.mocked(auth.getSetupStatus).mockResolvedValue({ initialised: true })
    vi.mocked(auth.me).mockRejectedValue(new ApiProblem('keeppix/unauthenticated', 'unauthenticated', 401))

    // A distinct target from the other tests: pushing to a path the router
    // is already sitting on is a same-route no-op in vue-router and would
    // never re-run the guard, making this test pass for the wrong reason.
    await router.push('/search')
    expect(router.currentRoute.value.path).toBe('/login')
  })
})

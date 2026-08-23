import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { fetchBootstrap } from '@/api/bootstrap'

import { useShellStore } from './shell'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn()
}))

afterEach(() => vi.resetAllMocks())

describe('useShellStore', () => {
  it('starts unloaded with safe defaults', () => {
    setActivePinia(createPinia())
    const shell = useShellStore()
    expect(shell.loaded).toBe(false)
    expect(shell.folders).toEqual([])
    expect(shell.storage).toEqual({})
    expect(shell.badges).toEqual({ culling: 0, revision: 0 })
  })

  it('load() populates folders/storage/badges from the real bootstrap response and flips loaded', async () => {
    setActivePinia(createPinia())
    vi.mocked(fetchBootstrap).mockResolvedValue({
      user: {
        id: '1',
        username: 'admin',
        display_name: 'Admin',
        email: null,
        role: 'admin',
        locale: null
      },
      folders: [{ id: 'f1', library_id: 'l1', parent_id: null, name: 'Urbino', depth: 0 }],
      storage: { l1: { free_bytes: 1_000, total_bytes: 2_000 } },
      badges: { culling: 3, revision: 5 }
    })

    const shell = useShellStore()
    await shell.load()

    expect(shell.loaded).toBe(true)
    expect(shell.folders).toEqual([{ id: 'f1', library_id: 'l1', parent_id: null, name: 'Urbino', depth: 0 }])
    expect(shell.storage.l1).toEqual({ free_bytes: 1_000, total_bytes: 2_000 })
    expect(shell.badges).toEqual({ culling: 3, revision: 5 })
  })
})

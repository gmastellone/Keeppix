import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'

vi.mock('@/api/culling', () => ({
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null })),
  unvotedFlags: { rating: null, pick: 'none', color_label: null }
}))

const cullingApi = await import('@/api/culling')
const { useCullingStore } = await import('./culling')

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: `${id}${'a'.repeat(63)}`.slice(0, 64),
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
    width: 100,
    height: 100,
    thumbhash: null
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('culling store — navigation', () => {
  it('advances to the next photo after voting on the current one', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b'), photo('c')])

    expect(store.currentAsset?.id).toBe('a')
    store.rate('a', 4)
    expect(store.currentAsset?.id).toBe('b')
  })

  it('does not advance past the last photo', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    store.goTo(1)
    expect(store.currentAsset?.id).toBe('b')
    store.rate('b', 5)
    expect(store.currentAsset?.id).toBe('b')
  })

  it('does not advance when voting on a photo that is not the current one', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b'), photo('c')])

    store.rate('c', 3)
    expect(store.currentAsset?.id).toBe('a')
  })
})

describe('culling store — filters', () => {
  it('"rejects" shows exactly and only the rejected photos, nothing else', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b'), photo('c'), photo('d')])

    store.reject('b')
    store.reject('d')
    store.setFilter('rejects')

    const ids = store.order.map((id) => id)
    expect(ids.sort()).toEqual(['b', 'd'])
    expect(store.currentAsset && ['b', 'd'].includes(store.currentAsset.id)).toBe(true)
  })

  it('"picks" never shows a rejected photo', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])
    store.pick('a')
    store.reject('b')
    store.setFilter('picks')

    expect(store.order).toEqual(['a'])
  })

  it('"all" shows every photo regardless of vote', () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])
    store.reject('a')
    store.setFilter('rejects')
    store.setFilter('all')

    expect(store.order.sort()).toEqual(['a', 'b'])
  })
})

describe('culling store — resilient queue', () => {
  it('keeps a vote queued and retries it after a network failure, without losing it', async () => {
    const setFlagsMock = vi.mocked(cullingApi.setFlags)
    setFlagsMock.mockRejectedValueOnce(new Error('network down'))

    const store = useCullingStore()
    store.start([photo('a')])

    store.rate('a', 5)
    await vi.waitFor(() => expect(setFlagsMock).toHaveBeenCalledTimes(1))

    // La rete è caduta: il voto resta in coda, non è andato perso.
    expect(store.queue).toHaveLength(1)
    expect(store.queue[0]).toMatchObject({ assetId: 'a', flags: { rating: 5 } })

    setFlagsMock.mockResolvedValueOnce(null)
    await store.retryQueue()

    expect(setFlagsMock).toHaveBeenCalledTimes(2)
    expect(store.queue).toHaveLength(0)
  })

  it('does not drop a second vote queued while the first retry is still pending', async () => {
    const setFlagsMock = vi.mocked(cullingApi.setFlags)
    let releaseFirst: (() => void) | undefined
    setFlagsMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseFirst = () => resolve(null)
        })
    )

    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    store.rate('a', 2)
    await vi.waitFor(() => expect(setFlagsMock).toHaveBeenCalledTimes(1))
    store.rate('b', 3)

    expect(store.queue).toHaveLength(2)

    setFlagsMock.mockResolvedValue(null)
    releaseFirst?.()
    await vi.waitFor(() => expect(store.queue).toHaveLength(0))
    expect(setFlagsMock).toHaveBeenCalledTimes(2)
  })
})

describe('culling store — 1000-photo session', () => {
  it('starts a session of 1000 photos and votes through without stalling', () => {
    const store = useCullingStore()
    const photos = Array.from({ length: 1000 }, (_, i) => photo(`id-${i}`))
    const started = performance.now()
    store.start(photos)
    expect(store.order).toHaveLength(1000)
    expect(store.currentAsset?.id).toBe('id-0')

    for (let i = 0; i < 200; i++) {
      const id = store.currentAsset?.id
      expect(id).toBeTruthy()
      if (i % 2 === 0) store.pick(id!)
      else store.reject(id!)
    }
    const elapsed = performance.now() - started
    // Sessione da tastiera: 200 voti + start devono restare impercettibili.
    expect(elapsed).toBeLessThan(2000)
    expect(store.position).toBe(200)
    expect(Object.keys(store.flagsById)).toHaveLength(200)
  })
})

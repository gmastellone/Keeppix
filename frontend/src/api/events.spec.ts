import { afterEach, describe, expect, it, vi } from 'vitest'

import { startLiveEvents, type LiveMessage } from './events'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('startLiveEvents', () => {
  it('reconnects after the socket closes and keeps listening', async () => {
    vi.useFakeTimers()
    const sockets: FakeSocket[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({
        ok: true,
        json: async () => ({ ticket: 't1' })
      }))
    )
    vi.stubGlobal(
      'WebSocket',
      class {
        static instances = sockets
        onmessage: ((ev: { data: string }) => void) | null = null
        onopen: (() => void) | null = null
        onclose: (() => void) | null = null
        constructor() {
          sockets.push(this as unknown as FakeSocket)
        }
        close() {
          this.onclose?.()
        }
      }
    )

    const received: string[] = []
    const live = startLiveEvents((msg: LiveMessage) => {
      received.push(msg.type)
    })

    await vi.runOnlyPendingTimersAsync()
    expect(sockets.length).toBe(1)
    sockets[0]?.onopen?.()
    sockets[0]?.onmessage?.({
      data: JSON.stringify({ v: 1, type: 'resync', payload: {} })
    })
    expect(received).toEqual(['resync'])

    sockets[0]?.onclose?.()
    await vi.runOnlyPendingTimersAsync()
    expect(sockets.length).toBeGreaterThan(1)

    live.close()
  })
})

type FakeSocket = {
  onmessage: ((ev: { data: string }) => void) | null
  onopen: (() => void) | null
  onclose: (() => void) | null
  close: () => void
}

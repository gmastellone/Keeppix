import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from './client'
import { fetchGeometry } from './timeline'

/** 8-byte header (version=1, count) + `count` zeroed 6-byte records —
 * enough to decode; the record contents don't matter for these tests. */
function geometryBody(count: number): ArrayBuffer {
  const out = new Uint8Array(8 + count * 6)
  new DataView(out.buffer).setUint32(0, 1, true)
  new DataView(out.buffer).setUint32(4, count, true)
  return out.buffer
}

afterEach(() => vi.unstubAllGlobals())

describe('fetchGeometry', () => {
  it('returns the raw ArrayBuffer and etag on 200', async () => {
    const body = new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0]).buffer
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(body, { status: 200, headers: { etag: '"abc"' } }))
    )

    const result = await fetchGeometry()
    expect(new Uint8Array(result.buffer!)).toEqual(new Uint8Array(body))
    expect(result.etag).toBe('"abc"')
  })

  it('sends If-None-Match when an etag is passed, and passes bbox through as a query param', async () => {
    const fetchMock = vi.fn(async () => new Response(new ArrayBuffer(8), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await fetchGeometry('1,2,3,4', '"prev-etag"')

    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/v1/timeline/geometry?bbox=1%2C2%2C3%2C4')
    expect(init.headers).toMatchObject({ 'if-none-match': '"prev-etag"' })
    expect(init.credentials).toBe('same-origin')
  })

  it('returns a null buffer and echoes the etag back on 304, without re-downloading', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 304 })))

    const result = await fetchGeometry(undefined, '"same-etag"')
    expect(result).toEqual({ buffer: null, etag: '"same-etag"', nextCursor: null })
  })

  it('throws ApiProblem on a problem+json error response, same as apiFetch', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ type: 'keeppix/forbidden', title: 'Forbidden', status: 403 }),
            { status: 403, headers: { 'content-type': 'application/problem+json' } }
          )
      )
    )

    await expect(fetchGeometry()).rejects.toMatchObject({ type: 'keeppix/forbidden', status: 403 })
    await expect(fetchGeometry()).rejects.toBeInstanceOf(ApiProblem)
  })

  it('sends limit/cursor as query params and never sends If-None-Match on a paged request', async () => {
    const fetchMock = vi.fn(async () => new Response(geometryBody(0), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await fetchGeometry('1,2,3,4', '"an-etag-that-must-be-ignored"', {
      limit: 4000,
      cursor: '2026-01-01T00:00:00.000000Z,abc'
    })

    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe(
      '/api/v1/timeline/geometry?bbox=1%2C2%2C3%2C4&limit=4000&cursor=2026-01-01T00%3A00%3A00.000000Z%2Cabc'
    )
    expect(init.headers).not.toHaveProperty('if-none-match')
  })

  it('reads the next-page cursor from the response header', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(geometryBody(1), {
            status: 200,
            headers: { 'x-keeppix-geometry-cursor': '2026-01-01T00:00:00.000000Z,abc' }
          })
      )
    )

    const result = await fetchGeometry(undefined, undefined, { limit: 1 })
    expect(result.nextCursor).toBe('2026-01-01T00:00:00.000000Z,abc')
  })
})

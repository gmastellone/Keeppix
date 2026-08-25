import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from './client'
import { fetchGeometry } from './timeline'

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
    expect(result).toEqual({ buffer: null, etag: '"same-etag"' })
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
})

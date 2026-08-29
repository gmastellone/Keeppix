import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem, apiFetch } from './client'

afterEach(() => vi.unstubAllGlobals())

function mockResponse(status: number, body: unknown, contentType: string) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': contentType }
      })
    )
  )
}

describe('apiFetch', () => {
  it('returns the body on a successful response', async () => {
    mockResponse(200, { user: { username: 'giovanni' } }, 'application/json')
    await expect(apiFetch('/api/v1/auth/me')).resolves.toEqual({
      user: { username: 'giovanni' }
    })
  })

  it('throws ApiProblem with the stable code', async () => {
    mockResponse(
      401,
      { type: 'keeppix/invalid-credentials', title: 'Invalid credentials', status: 401 },
      'application/problem+json'
    )

    await expect(apiFetch('/api/v1/auth/login')).rejects.toMatchObject({
      type: 'keeppix/invalid-credentials',
      status: 401
    })
  })

  it('throws a generic ApiProblem if the body is not problem+json', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('boom', { status: 502 })))

    const error = await apiFetch('/api/v1/auth/me').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiProblem)
    expect((error as ApiProblem).status).toBe(502)
  })

  /**
   * The backend requires `x-keeppix-client` on mutations and responds
   * `403 keeppix/csrf-check-failed` if it's missing. If someone removed
   * the header from `apiFetch`, every login, setup, and logout would
   * stop working: this test catches that without having to spin up the
   * backend.
   */
  it('sends the JSON content-type and the required custom header on mutations', async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)

    await apiFetch('/api/v1/auth/logout', { method: 'POST' })

    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(init.headers).toMatchObject({
      'content-type': 'application/json',
      'x-keeppix-client': 'web'
    })
    expect(init.credentials).toBe('same-origin')
  })

  it('returns null on 204', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 204 })))
    await expect(apiFetch('/api/v1/auth/refresh')).resolves.toBeNull()
  })
})

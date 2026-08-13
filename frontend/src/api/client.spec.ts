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
  it('restituisce il corpo su risposta positiva', async () => {
    mockResponse(200, { user: { username: 'giovanni' } }, 'application/json')
    await expect(apiFetch('/api/v1/auth/me')).resolves.toEqual({
      user: { username: 'giovanni' }
    })
  })

  it('lancia ApiProblem con il codice stabile', async () => {
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

  it('lancia ApiProblem generico se il corpo non è problem+json', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('boom', { status: 502 })))

    const error = await apiFetch('/api/v1/auth/me').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiProblem)
    expect((error as ApiProblem).status).toBe(502)
  })

  it('restituisce null su 204', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 204 })))
    await expect(apiFetch('/api/v1/auth/refresh')).resolves.toBeNull()
  })
})

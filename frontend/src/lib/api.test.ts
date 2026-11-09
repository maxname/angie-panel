import { afterEach, describe, expect, it, vi } from 'vitest'

import { api, ApiError } from './api'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('api client', () => {
  it('sends the X-AP-Request header and same-origin credentials on mutations', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    await api.login({ email: 'admin@example.com', password: 'secret' })

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/api/auth/login')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('same-origin')
    expect(init.headers).toMatchObject({
      'X-AP-Request': '1',
      'Content-Type': 'application/json',
    })
    expect(init.body).toBe(
      JSON.stringify({ email: 'admin@example.com', password: 'secret' }),
    )
  })

  it('sends the X-AP-Request header on body-less mutations too', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({
        timestamp: 1751700000,
        ok: true,
        exit_code: 0,
        output: 'syntax is ok',
        ran_via: 'systemd',
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    await api.runConfigtest()

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/api/system/configtest')
    expect(init.method).toBe('POST')
    expect(init.headers).toMatchObject({ 'X-AP-Request': '1' })
  })

  it('does not send the X-AP-Request header on GET requests', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({ setup_required: false, authenticated: true }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const state = await api.getAuthState()

    expect(state).toEqual({ setup_required: false, authenticated: true })
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/api/auth/state')
    expect(init.method).toBe('GET')
    expect(init.credentials).toBe('same-origin')
    expect(init.headers).not.toMatchObject({ 'X-AP-Request': '1' })
  })

  it('throws a typed ApiError parsed from the error response body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(
        { error: { code: 'invalid_credentials', message: 'Invalid email or password' } },
        401,
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    const failure = api.login({ email: 'admin@example.com', password: 'wrong' })

    await expect(failure).rejects.toBeInstanceOf(ApiError)
    await expect(failure).rejects.toMatchObject({
      status: 401,
      code: 'invalid_credentials',
      message: 'Invalid email or password',
    })
  })
})

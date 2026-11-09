import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Ban } from '@/lib/api'

import { BlocklistPage } from './blocklist'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const ban: Ban = {
  id: 1,
  address: '203.0.113.7',
  reason: 'brute force',
  created_at: 1751700000,
}

/** Path-aware fetch: /api/geo returns the policy, /api/bans returns the bans. */
function routedFetch(bans: Ban[], geo = { mode: 'off', countries: [] }) {
  return vi.fn((input: string) => {
    if (input === '/api/geo') {
      return Promise.resolve(jsonResponse(geo))
    }
    return Promise.resolve(jsonResponse({ bans }))
  })
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})
afterEach(() => vi.unstubAllGlobals())

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
}

describe('blocklist page', () => {
  it('lists blocked addresses', async () => {
    vi.stubGlobal('fetch', routedFetch([ban]))

    render(
      <QueryClientProvider client={makeClient()}>
        <BlocklistPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('203.0.113.7')).toBeInTheDocument()
    expect(screen.getByText('brute force')).toBeInTheDocument()
  })

  it('shows an empty state', async () => {
    vi.stubGlobal('fetch', routedFetch([]))

    render(
      <QueryClientProvider client={makeClient()}>
        <BlocklistPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText(/No blocked IPs/i)).toBeInTheDocument()
  })

  it('surfaces a 409 already_banned error and posts to /api/bans', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/bans' && init?.method === 'POST') {
        return Promise.resolve(
          jsonResponse(
            { error: { code: 'already_banned', message: '203.0.113.7 is already on the blocklist' } },
            409,
          ),
        )
      }
      if (input === '/api/geo') {
        return Promise.resolve(jsonResponse({ mode: 'off', countries: [] }))
      }
      return Promise.resolve(jsonResponse({ bans: [] }))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <BlocklistPage />
      </QueryClientProvider>,
    )

    await screen.findByText(/No blocked IPs/i)
    await user.type(screen.getByLabelText('IP or CIDR'), '203.0.113.7')
    await user.click(screen.getByRole('button', { name: 'Block' }))

    expect(
      await screen.findByText('203.0.113.7 is already on the blocklist'),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/bans',
      expect.objectContaining({ method: 'POST' }),
    )
  })

  it('saves a country deny policy via PUT /api/geo', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/geo' && init?.method === 'PUT') {
        return Promise.resolve(jsonResponse({ mode: 'deny', countries: ['RU'] }))
      }
      if (input === '/api/geo') {
        return Promise.resolve(jsonResponse({ mode: 'off', countries: [] }))
      }
      return Promise.resolve(jsonResponse({ bans: [] }))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <BlocklistPage />
      </QueryClientProvider>,
    )

    // Set mode → "Block listed countries" (the first combobox is the geo mode).
    await user.click(await screen.findByLabelText('Mode'))
    await user.click(
      await screen.findByRole('option', { name: 'Block listed countries' }),
    )
    // Add Russia via the country picker.
    await user.click(screen.getByLabelText('Add country'))
    await user.click(await screen.findByRole('option', { name: 'Russia (RU)' }))
    // Save (the geo card's Save button).
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const put = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/geo' && init?.method === 'PUT',
    )
    expect(put).toBeTruthy()
    const body = JSON.parse(String((put![1] as RequestInit).body))
    expect(body).toEqual({ mode: 'deny', countries: ['RU'] })
  })
})

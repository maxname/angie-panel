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
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ bans: [ban] })))

    render(
      <QueryClientProvider client={makeClient()}>
        <BlocklistPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('203.0.113.7')).toBeInTheDocument()
    expect(screen.getByText('brute force')).toBeInTheDocument()
  })

  it('shows an empty state', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ bans: [] })))

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
})

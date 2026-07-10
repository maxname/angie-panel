import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { DeadHost } from '@/lib/api'

import { DeadHostEditorForm, DeadHostsPage } from './dead-hosts'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const sampleDead: DeadHost = {
  id: 1,
  domains: ['parked.example.com'],
  certificate_id: null,
  force_ssl: false,
  hsts: false,
  hsts_subdomains: false,
  http2: true,
  advanced_snippet: null,
  enabled: false,
  created_at: 1751700000,
  updated_at: 1751700000,
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
}

describe('404 hosts page', () => {
  it('renders the table from a mocked fetch with the 404 note', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(jsonResponse({ dead_hosts: [sampleDead] }))
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <DeadHostsPage />
      </QueryClientProvider>,
    )

    // Domain renders as a badge.
    expect(await screen.findByText('parked.example.com')).toBeInTheDocument()
    // Every 404 host advertises the same behaviour.
    expect(screen.getByText('Returns 404')).toBeInTheDocument()
    // Disabled status pill.
    expect(screen.getByText('Disabled')).toBeInTheDocument()

    expect(fetchMock).toHaveBeenCalledWith('/api/dead-hosts', expect.anything())
  })

  it('shows an empty state when there are no 404 hosts', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ dead_hosts: [] })),
    )

    render(
      <QueryClientProvider client={makeClient()}>
        <DeadHostsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText(/No 404 hosts yet/i)).toBeInTheDocument()
  })
})

describe('404 host editor', () => {
  it('blocks submit and shows an error when no domains are entered', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <DeadHostEditorForm host={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Add at least one domain name.'),
    ).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalledWith(
      '/api/dead-hosts',
      expect.anything(),
    )
  })

  it('surfaces a 409 domain_conflict message verbatim', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/dead-hosts') {
        return Promise.resolve(
          jsonResponse(
            {
              error: {
                code: 'domain_conflict',
                message: 'parked.example.com already used by proxy host “app”',
              },
            },
            409,
          ),
        )
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <DeadHostEditorForm host={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.type(screen.getByLabelText('Domain names'), 'parked.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(
        'parked.example.com already used by proxy host “app”',
      ),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/dead-hosts',
      expect.objectContaining({ method: 'POST' }),
    )
  })
})

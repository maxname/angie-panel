import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { RedirectHost } from '@/lib/api'

import { RedirectHostEditorForm, RedirectHostsPage } from './redirect-hosts'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const sampleRedirect: RedirectHost = {
  id: 1,
  domains: ['old.example.com'],
  forward_scheme: 'https',
  forward_domain: 'new.example.com',
  forward_http_code: 301,
  preserve_path: true,
  certificate_id: 5,
  force_ssl: true,
  hsts: false,
  hsts_subdomains: false,
  http2: true,
  block_exploits: false,
  advanced_snippet: null,
  enabled: true,
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

describe('redirection hosts page', () => {
  it('renders the table from a mocked fetch', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(jsonResponse({ redirect_hosts: [sampleRedirect] }))
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <RedirectHostsPage />
      </QueryClientProvider>,
    )

    // Domain renders as a badge.
    expect(await screen.findByText('old.example.com')).toBeInTheDocument()
    // Target combines status code, scheme and forward domain.
    expect(
      screen.getByText('301 → https://new.example.com'),
    ).toBeInTheDocument()
    // Enabled status pill.
    expect(screen.getByText('Enabled')).toBeInTheDocument()
    // The header "SSL" plus the attached-certificate badge.
    expect(screen.getAllByText('SSL').length).toBeGreaterThanOrEqual(2)

    expect(fetchMock).toHaveBeenCalledWith('/api/redirect-hosts', expect.anything())
  })

  it('shows an empty state when there are no redirection hosts', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse({ redirect_hosts: [] })),
    )

    render(
      <QueryClientProvider client={makeClient()}>
        <RedirectHostsPage />
      </QueryClientProvider>,
    )

    expect(
      await screen.findByText(/No redirection hosts yet/i),
    ).toBeInTheDocument()
  })
})

describe('redirection host editor', () => {
  it('blocks submit and shows an error when no domains are entered', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <RedirectHostEditorForm host={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Add at least one domain name.'),
    ).toBeInTheDocument()
    // Nothing was submitted to the server.
    expect(fetchMock).not.toHaveBeenCalledWith(
      '/api/redirect-hosts',
      expect.anything(),
    )
  })

  it('surfaces a 409 domain_conflict message verbatim', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/redirect-hosts') {
        return Promise.resolve(
          jsonResponse(
            {
              error: {
                code: 'domain_conflict',
                message: 'old.example.com already used by proxy host “app”',
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
        <RedirectHostEditorForm host={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    // A valid domain chip and a forward domain get us past client validation.
    await user.type(screen.getByLabelText('Domain names'), 'old.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.type(
      screen.getByLabelText('Forward domain'),
      'new.example.com',
    )
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(
        'old.example.com already used by proxy host “app”',
      ),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/redirect-hosts',
      expect.objectContaining({ method: 'POST' }),
    )
  })
})

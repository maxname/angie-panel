import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Host } from '@/lib/api'

import { HostsPage } from './hosts'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const sampleHost: Host = {
  id: 1,
  domains: ['example.com', 'www.example.com'],
  forward_scheme: 'http',
  forward_host: '10.0.0.5',
  forward_port: 8080,
  websockets_upgrade: true,
  block_exploits: true,
  cache_assets: false,
  http2: true,
  http3: false,
  force_ssl: false,
  health_checks: [],
  hsts: false,
  hsts_subdomains: false,
  trust_forwarded_proto: false,
  certificate_id: null,
  access_list_id: null,
  locations: [],
  advanced_snippet: null,
  rate_limit: { enabled: false, rps: 0, burst: 0, nodelay: false, conn: 0 },
  upstream: {
    servers: [],
    method: 'round_robin',
    primary_weight: 1,
    max_fails: 1,
    fail_timeout_secs: 10,
  },
  mtls: { ca_pem: null, optional: false },
  forward_auth: {
    enabled: false,
    verify_url: '',
    sign_in_url: null,
    copy_headers: [],
  },
  custom_headers: [],
  maintenance: { enabled: false, title: '', message: '' },
  gzip: { enabled: false, comp_level: 0, min_length: 0, types: [] },
  error_pages: {
    not_found: { enabled: false, title: '', message: '' },
    server_error: { enabled: false, title: '', message: '' },
  },
  proxy_tuning: {
    client_max_body_size: '',
    connect_timeout_secs: 0,
    read_timeout_secs: 0,
    send_timeout_secs: 0,
    disable_buffering: false,
  },
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

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <HostsPage />
    </QueryClientProvider>,
  )
}

describe('proxy hosts page', () => {
  it('sorts by domain, and reverses on a second click', async () => {
    // Insertion order from the API is deliberately not alphabetical, so a table
    // that ignored the sort would still show zeta first and pass nothing.
    const rows = ['zeta.example.com', 'alpha.example.com', 'host9.example.com', 'host10.example.com']
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({
        hosts: rows.map((d, i) => ({ ...sampleHost, id: i + 1, domains: [d] })),
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    renderPage()
    await screen.findByText('alpha.example.com')

    // Cards, not table rows: each host renders its domain as a link, and their
    // DOM order is the list order.
    const shown = () =>
      screen
        .getAllByRole('link')
        .map((el) => el.textContent?.trim())
        .filter((t): t is string => !!t && /\.example\.com$/.test(t))

    // host9 before host10: plain string order would put 10 first.
    expect(shown()).toEqual([
      'alpha.example.com',
      'host9.example.com',
      'host10.example.com',
      'zeta.example.com',
    ])

    await userEvent.click(screen.getByRole('button', { name: /sort/i }))

    expect(shown()).toEqual([
      'zeta.example.com',
      'host10.example.com',
      'host9.example.com',
      'alpha.example.com',
    ])
  })

  it('renders the hosts table from a mocked fetch', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(jsonResponse({ hosts: [sampleHost] }))
    vi.stubGlobal('fetch', fetchMock)

    renderPage()

    // Domains render as badges.
    expect(await screen.findByText('example.com')).toBeInTheDocument()
    expect(screen.getByText('www.example.com')).toBeInTheDocument()
    // Forward target is "scheme://host:port".
    expect(screen.getByText('http://10.0.0.5:8080')).toBeInTheDocument()
    // Enabled status pill.
    expect(screen.getByText('Enabled')).toBeInTheDocument()

    expect(fetchMock).toHaveBeenCalledWith('/api/hosts', expect.anything())
  })

  it('shows an empty state when there are no hosts', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ hosts: [] })))

    renderPage()

    expect(
      await screen.findByText(/No proxy hosts yet/i),
    ).toBeInTheDocument()
  })
})

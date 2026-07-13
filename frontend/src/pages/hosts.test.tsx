import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
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

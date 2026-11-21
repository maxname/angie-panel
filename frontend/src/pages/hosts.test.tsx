import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
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
  upstream_pool_id: null,
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

/** Every request answered with the same body — each with a Response of its
 *  own, since a body can be read only once and the page makes several
 *  requests (hosts, certificates, pools, templates) in no promised order. */
function answerAll(body: unknown) {
  return vi.fn(() => Promise.resolve(jsonResponse(body)))
}

/** The page's lists answered by address (so an editor opened from the page
 *  gets well-formed answers too); anything else fails the request. */
function listsApi(overrides: Record<string, unknown> = {}) {
  const lists: Record<string, unknown> = {
    '/api/hosts': { hosts: [sampleHost] },
    '/api/certificates': { certificates: [] },
    '/api/access-lists': { access_lists: [] },
    '/api/upstream-pools': { upstream_pools: [] },
    '/api/templates': { templates: [] },
    ...overrides,
  }
  return vi.fn((input: string) =>
    input in lists
      ? Promise.resolve(jsonResponse(lists[input]))
      : Promise.reject(new Error(`unexpected fetch ${input}`)),
  )
}

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
    const fetchMock = answerAll({
      hosts: rows.map((d, i) => ({ ...sampleHost, id: i + 1, domains: [d] })),
    })
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
    const fetchMock = answerAll({ hosts: [sampleHost] })
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

  it('names the shared pool for a host that has no target of its own', async () => {
    // A host on a pool is stored with an empty forward host and port 0;
    // printing that would read "http://:0".
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/hosts') {
        return Promise.resolve(
          jsonResponse({
            hosts: [
              {
                ...sampleHost,
                forward_scheme: 'https',
                forward_host: '',
                forward_port: 0,
                upstream_pool_id: 3,
              },
            ],
          }),
        )
      }
      if (input === '/api/upstream-pools') {
        return Promise.resolve(
          jsonResponse({ upstream_pools: [{ id: 3, name: 'apps', servers: [] }] }),
        )
      }
      return Promise.resolve(jsonResponse({ certificates: [] }))
    })
    vi.stubGlobal('fetch', fetchMock)

    renderPage()

    expect(await screen.findByText('https://apps')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: 'Shared upstream pool' })).toBeInTheDocument()
    expect(screen.queryByText(/:0/)).toBeNull()
  })

  it('disables the ticked hosts with one request each, in list order', async () => {
    // Three hosts; the middle one is already off. The API is stateful enough
    // to show the refetch picking the change up.
    let hosts = [
      { ...sampleHost, id: 1, domains: ['a.example.com'], enabled: true },
      { ...sampleHost, id: 2, domains: ['b.example.com'], enabled: false },
      { ...sampleHost, id: 3, domains: ['c.example.com'], enabled: true },
    ]
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const toggle = /^\/api\/hosts\/(\d+)\/(enable|disable)$/.exec(input)
      if (toggle && init?.method === 'POST') {
        const id = Number(toggle[1])
        hosts = hosts.map((h) =>
          h.id === id ? { ...h, enabled: toggle[2] === 'enable' } : h,
        )
        return Promise.resolve(jsonResponse({ ok: true }))
      }
      if (input === '/api/hosts') {
        return Promise.resolve(jsonResponse({ hosts }))
      }
      if (input === '/api/upstream-pools') {
        return Promise.resolve(jsonResponse({ upstream_pools: [] }))
      }
      return Promise.resolve(jsonResponse({ certificates: [] }))
    })
    vi.stubGlobal('fetch', fetchMock)
    const user = userEvent.setup()

    renderPage()
    await screen.findByText('a.example.com')
    expect(screen.getAllByText('Enabled')).toHaveLength(2)

    await user.click(screen.getByRole('checkbox', { name: 'Select all' }))
    const bar = screen.getByRole('toolbar')
    expect(within(bar).getByText('Selected: 3')).toBeInTheDocument()
    await user.click(within(bar).getByRole('button', { name: 'Disable' }))

    // The same single-host endpoint the row menu uses, once per host that was
    // on — never for the one already off — and in the order of the list.
    await vi.waitFor(() => expect(screen.queryByText('Enabled')).toBeNull())
    const posts = fetchMock.mock.calls
      .filter(([, init]) => init?.method === 'POST')
      .map(([url]) => url)
    expect(posts).toEqual(['/api/hosts/1/disable', '/api/hosts/3/disable'])
    expect(screen.getAllByText('Disabled')).toHaveLength(3)
    // Done: nothing is left ticked and the bar is gone.
    expect(screen.queryByRole('toolbar')).toBeNull()
  })

  it('offers the proxy host templates next to Add, and starts a host from the one picked', async () => {
    vi.stubGlobal(
      'fetch',
      listsApi({
        '/api/templates': {
          templates: [
            {
              id: 1,
              kind: 'proxy_host',
              name: 'Web app',
              // A host's settings without its domains — here, the sample's.
              config: { ...sampleHost, forward_host: '10.9.9.9', new_certificate: null },
            },
            // Not a kind this page can create: must not be offered.
            { id: 2, kind: 'stream', name: 'Database', config: {} },
          ],
        },
      }),
    )
    const user = userEvent.setup()

    renderPage()
    await screen.findByText('example.com')

    await user.click(screen.getByRole('button', { name: 'From template' }))
    expect(await screen.findByRole('menuitem', { name: 'Web app' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Database' })).toBeNull()

    await user.click(screen.getByRole('menuitem', { name: 'Web app' }))
    // The ordinary "new host" editor — not a template editor — already
    // carrying the template's settings, with the domains still to add.
    const dialog = await screen.findByRole('dialog', { name: 'New proxy host' })
    expect(within(dialog).getByLabelText('Forward host')).toHaveValue('10.9.9.9')
    expect(within(dialog).getByLabelText('Websockets support')).toBeChecked()
    expect(within(dialog).getByLabelText('Domain names')).toHaveValue('')
  })

  it('says where templates come from when there are none', async () => {
    vi.stubGlobal('fetch', listsApi())
    const user = userEvent.setup()

    renderPage()
    await screen.findByText('example.com')
    await user.click(screen.getByRole('button', { name: 'From template' }))

    expect(await screen.findByText(/No templates yet/)).toBeInTheDocument()
  })

  it('turns a host into a new template from its menu', async () => {
    vi.stubGlobal('fetch', listsApi())
    const user = userEvent.setup()

    renderPage()
    await screen.findByText('example.com')
    await user.click(screen.getByRole('button', { name: 'Actions' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Save as template' }))

    // The template editor, seeded from the host: its target, not its domains.
    const dialog = await screen.findByRole('dialog', {
      name: 'New proxy host template',
    })
    expect(within(dialog).getByLabelText('Forward host')).toHaveValue('10.0.0.5')
    expect(within(dialog).getByLabelText('Template name')).toHaveValue('')
    expect(within(dialog).queryByLabelText('Domain names')).toBeNull()
  })

  it('shows an empty state when there are no hosts', async () => {
    vi.stubGlobal('fetch', answerAll({ hosts: [] }))

    renderPage()

    expect(
      await screen.findByText(/No proxy hosts yet/i),
    ).toBeInTheDocument()
  })
})

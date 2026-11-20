import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { HostTemplate, StreamTemplate, Template } from '@/lib/api'

import { TemplatesPage } from './templates'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const hostSettings: HostTemplate['config'] = {
  forward_scheme: 'http',
  forward_host: '',
  forward_port: 8080,
  websockets_upgrade: false,
  block_exploits: false,
  cache_assets: false,
  http2: true,
  http3: false,
  force_ssl: true,
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
  forward_auth: { enabled: false, verify_url: '', sign_in_url: null, copy_headers: [] },
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
  new_certificate: null,
}

const stamps = { created_at: 1751700000, updated_at: 1751700000 }

// Open target, a certificate requested per host.
const publicSite: HostTemplate = {
  id: 1,
  kind: 'proxy_host',
  name: 'Public site',
  config: {
    ...hostSettings,
    new_certificate: {
      challenge: 'http',
      dns_provider: null,
      ca: null,
      key_type: null,
      email: null,
      staging: false,
    },
  },
  ...stamps,
}
// On a shared pool, behind an existing (wildcard) certificate.
const internalApp: HostTemplate = {
  id: 2,
  kind: 'proxy_host',
  name: 'Internal app',
  config: { ...hostSettings, upstream_pool_id: 3, certificate_id: 4 },
  ...stamps,
}
const database: StreamTemplate = {
  id: 3,
  kind: 'stream',
  name: 'Database',
  config: {
    forward_host: '10.0.0.20',
    forward_port: 5432,
    tcp: true,
    udp: false,
    tls: 'none',
    certificate_id: null,
  },
  ...stamps,
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

/** A stateful stand-in for the API: lists what the page reads, deletes templates. */
function stubApi(initial: Template[]) {
  let templates = initial
  const fetchMock = vi.fn((input: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    const one = /^\/api\/templates\/(\d+)$/.exec(input)
    if (one && method === 'DELETE') {
      templates = templates.filter((tpl) => tpl.id !== Number(one[1]))
      return Promise.resolve(jsonResponse({ ok: true }))
    }
    const lists: Record<string, unknown> = {
      '/api/templates': { templates },
      '/api/certificates': {
        certificates: [{ id: 4, name: 'wild', domains: ['*.example.com'] }],
      },
      '/api/upstream-pools': {
        upstream_pools: [{ id: 3, name: 'apps', servers: [] }],
      },
      '/api/access-lists': { access_lists: [] },
    }
    return method === 'GET' && input in lists
      ? Promise.resolve(jsonResponse(lists[input]))
      : Promise.reject(new Error(`unexpected fetch ${method} ${input}`))
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <TemplatesPage />
    </QueryClientProvider>,
  )
}

const row = (name: string) => screen.getByText(name).closest('tr')!

describe('templates page', () => {
  it('lists templates of both kinds with where they point and how they get a certificate', async () => {
    stubApi([publicSite, internalApp, database])
    renderPage()

    await screen.findByText('Public site')
    // The target left open, and a certificate asked for per host.
    expect(within(row('Public site')).getByText('Proxy host')).toBeInTheDocument()
    expect(within(row('Public site')).getByText('set when creating')).toBeInTheDocument()
    expect(within(row('Public site')).getByText('requested per host')).toBeInTheDocument()

    // A pool and an existing certificate, by name rather than by id.
    expect(await within(row('Internal app')).findByText('apps')).toBeInTheDocument()
    expect(
      await within(row('Internal app')).findByText('*.example.com'),
    ).toBeInTheDocument()

    // A stream template: host:port, no scheme, no certificate.
    expect(within(row('Database')).getByText('Stream')).toBeInTheDocument()
    expect(within(row('Database')).getByText('10.0.0.20:5432')).toBeInTheDocument()
    expect(within(row('Database')).getByText('—')).toBeInTheDocument()
  })

  it('shows an empty state when there are no templates', async () => {
    stubApi([])
    renderPage()

    expect(await screen.findByText(/No templates yet/i)).toBeInTheDocument()
  })

  it('opens the editor of the right kind, on a blank or on the template', async () => {
    const user = userEvent.setup()
    stubApi([publicSite, database])
    renderPage()
    await screen.findByText('Public site')

    // New: the kind is chosen up front, since the two have different settings.
    await user.click(screen.getByRole('button', { name: 'Add template' }))
    await user.click(
      await screen.findByRole('menuitem', { name: 'Stream template' }),
    )
    let dialog = await screen.findByRole('dialog', { name: 'New stream template' })
    expect(within(dialog).getByLabelText('Template name')).toHaveValue('')
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    // Edit: a stream template opens in the stream editor, filled in…
    await user.click(within(row('Database')).getByRole('button', { name: 'Actions' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Edit' }))
    dialog = await screen.findByRole('dialog', { name: 'Edit stream template' })
    expect(within(dialog).getByLabelText('Template name')).toHaveValue('Database')
    expect(within(dialog).getByLabelText('Forward host')).toHaveValue('10.0.0.20')
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    // …and a host template in the host editor.
    await user.click(
      within(row('Public site')).getByRole('button', { name: 'Actions' }),
    )
    await user.click(await screen.findByRole('menuitem', { name: 'Edit' }))
    dialog = await screen.findByRole('dialog', { name: 'Edit proxy host template' })
    expect(within(dialog).getByLabelText('Template name')).toHaveValue('Public site')
  })

  it('deletes a template', async () => {
    const user = userEvent.setup()
    const fetchMock = stubApi([publicSite, database])
    renderPage()
    await screen.findByText('Public site')

    await user.click(within(row('Database')).getByRole('button', { name: 'Actions' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Delete' }))
    const confirm = await screen.findByRole('dialog', { name: 'Delete template?' })
    // Deleting a template takes nothing with it.
    expect(
      within(confirm).getByText(/already created from it are not affected/),
    ).toBeInTheDocument()
    await user.click(within(confirm).getByRole('button', { name: 'Delete' }))

    await vi.waitFor(() => expect(screen.queryByText('Database')).toBeNull())
    expect(screen.getByText('Public site')).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/templates/3',
      expect.objectContaining({ method: 'DELETE' }),
    )
  })
})

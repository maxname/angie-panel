import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { SniRouter } from '@/lib/api'

import { SniRoutersPage } from './sni-routers'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const sampleRouter: SniRouter = {
  id: 1,
  name: 'edge',
  incoming_port: 443,
  routes: [
    { sni: 'app.example.com', forward_host: '10.0.0.10', forward_port: 443 },
    { sni: '*.internal.example.com', forward_host: '10.0.0.20', forward_port: 8443 },
  ],
  default_host: '10.0.0.1',
  default_port: 443,
  enabled: true,
  created_at: 1751700000,
  updated_at: 1751700000,
}

function routedFetch(routers: SniRouter[], contextActive: boolean) {
  return vi.fn((input: string) => {
    if (input === '/api/sni-routers') {
      return Promise.resolve(jsonResponse({ sni_routers: routers }))
    }
    if (input === '/api/dashboard') {
      return Promise.resolve(
        jsonResponse({
          angie: { up: false, version: null, generation: null, load_time: null, connections: null },
          hosts: [],
          certificates: [],
          streams: { configured: 0, enabled: 0, context_active: contextActive },
          drift: { detected: false, foreign_files: [] },
          pending_changes: false,
          alerts: [],
        }),
      )
    }
    return Promise.reject(new Error(`unexpected fetch ${input}`))
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

function renderPage() {
  return render(
    <QueryClientProvider client={makeClient()}>
      <SniRoutersPage />
    </QueryClientProvider>,
  )
}

describe('SNI routers page', () => {
  it('renders the table from a mocked fetch', async () => {
    vi.stubGlobal('fetch', routedFetch([sampleRouter], true))
    renderPage()

    expect(await screen.findByText('edge')).toBeInTheDocument()
    expect(screen.getByText('443')).toBeInTheDocument()
    // 2 routes + a catch-all = 3 backends.
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('Enabled')).toBeInTheDocument()
    // Context active → no enable banner.
    expect(screen.queryByText(/stream context is off/i)).not.toBeInTheDocument()
  })

  it('shows the enable banner when the stream context is off', async () => {
    vi.stubGlobal('fetch', routedFetch([sampleRouter], false))
    renderPage()

    expect(
      await screen.findByRole('button', { name: 'Enable streams' }),
    ).toBeInTheDocument()
  })

  it('shows an empty state when there are no routers', async () => {
    vi.stubGlobal('fetch', routedFetch([], true))
    renderPage()

    expect(await screen.findByText(/No SNI routers yet/i)).toBeInTheDocument()
  })

  it('preserves the disabled state when editing a router', async () => {
    const user = userEvent.setup()
    const disabled: SniRouter = { ...sampleRouter, enabled: false }
    const puts: Array<{ enabled?: boolean }> = []
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (input === '/api/sni-routers' && method === 'GET') {
        return Promise.resolve(jsonResponse({ sni_routers: [disabled] }))
      }
      if (input === '/api/dashboard') {
        return Promise.resolve(
          jsonResponse({
            angie: { up: false, version: null, generation: null, load_time: null, connections: null },
            hosts: [],
            certificates: [],
            streams: { configured: 1, enabled: 0, context_active: true },
            drift: { detected: false, foreign_files: [] },
            pending_changes: false,
            alerts: [],
          }),
        )
      }
      if (input === '/api/sni-routers/1' && method === 'PUT') {
        puts.push(JSON.parse(init?.body as string))
        return Promise.resolve(jsonResponse({ ...disabled }))
      }
      return Promise.reject(new Error(`unexpected fetch ${method} ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderPage()

    // Open the row actions, then Edit.
    await user.click(await screen.findByRole('button', { name: 'Actions' }))
    await user.click(await screen.findByText('Edit'))
    // Save without touching the enabled toggle.
    await user.click(await screen.findByRole('button', { name: 'Save' }))

    await waitFor(() => expect(puts).toHaveLength(1))
    // The editor form must resend enabled:false, not silently re-enable it.
    expect(puts[0].enabled).toBe(false)
  })

  it('warns about the 443 conflict in the editor', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', routedFetch([], true))
    renderPage()

    await user.click(await screen.findByRole('button', { name: 'Add SNI router' }))
    // The default port is 443, so the proxy-host conflict warning shows.
    expect(
      await screen.findByText(/Port 443 is also used by proxy hosts/i),
    ).toBeInTheDocument()
  })
})

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import { UpstreamPoolEditorForm } from '@/components/upstream-pools/upstream-pool-editor'
import i18n from '@/i18n'
import type { UpstreamPool } from '@/lib/api'

import { DeleteUpstreamPoolDialog, UpstreamPoolsPage } from './upstream-pools'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const apps: UpstreamPool = {
  id: 3,
  name: 'apps',
  method: 'least_conn',
  max_fails: 2,
  fail_timeout_secs: 20,
  servers: [
    { host: '10.0.0.2', port: 8080, weight: 3, backup: false, down: false },
    { host: '10.0.0.3', port: 8080, weight: 1, backup: true, down: false },
    { host: 'old.internal', port: 9000, weight: 1, backup: false, down: true },
  ],
  created_at: 1751700000,
  updated_at: 1751700000,
}

const parked: UpstreamPool = {
  ...apps,
  id: 4,
  name: 'parked',
  method: 'round_robin',
  servers: [{ host: '10.0.9.9', port: 80, weight: 1, backup: false, down: false }],
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

function renderForm(pool: UpstreamPool | null, onSaved = vi.fn()) {
  render(
    <QueryClientProvider client={makeClient()}>
      <UpstreamPoolEditorForm pool={pool} onDone={() => {}} onSaved={onSaved} />
    </QueryClientProvider>,
  )
  return onSaved
}

describe('upstream pools page', () => {
  it('lists each pool with its servers, method and the hosts that use it', async () => {
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/upstream-pools') {
        return Promise.resolve(jsonResponse({ upstream_pools: [apps, parked] }))
      }
      if (input === '/api/hosts') {
        // Only what the page reads: which pool each host proxies to.
        return Promise.resolve(
          jsonResponse({
            hosts: [
              { id: 1, domains: ['a.example.com'], upstream_pool_id: 3 },
              { id: 2, domains: ['b.example.com'], upstream_pool_id: 3 },
              { id: 3, domains: ['own.example.com'], upstream_pool_id: null },
            ],
          }),
        )
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <UpstreamPoolsPage />
      </QueryClientProvider>,
    )

    const appsRow = (await screen.findByText('apps')).closest('tr')!
    // Servers with what sets each apart: weight, backup, down.
    expect(within(appsRow).getByText('10.0.0.2:8080')).toBeInTheDocument()
    expect(within(appsRow).getByText('weight 3')).toBeInTheDocument()
    expect(within(appsRow).getByText('backup')).toBeInTheDocument()
    expect(within(appsRow).getByText('old.internal:9000')).toHaveClass('line-through')
    expect(within(appsRow).getByText('least_conn')).toBeInTheDocument()
    // Two hosts proxy to it; their names are a hover away.
    expect(await within(appsRow).findByText('2')).toHaveAttribute(
      'title',
      'a.example.com, b.example.com',
    )

    const parkedRow = screen.getByText('parked').closest('tr')!
    expect(within(parkedRow).getByText('not used')).toBeInTheDocument()
  })

  it('shows an empty state when there are no pools', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string) =>
        Promise.resolve(
          jsonResponse(
            input === '/api/hosts' ? { hosts: [] } : { upstream_pools: [] },
          ),
        ),
      ),
    )

    render(
      <QueryClientProvider client={makeClient()}>
        <UpstreamPoolsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText(/No upstream pools yet/i)).toBeInTheDocument()
  })
})

describe('upstream pool editor', () => {
  it('creates a pool and hands the saved one back', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((_input: string, init?: RequestInit) =>
      Promise.resolve(
        jsonResponse({ ...JSON.parse(String(init?.body)), id: 9 }),
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    const onSaved = renderForm(null)

    // A new pool opens with its first server row already there.
    await user.type(screen.getByLabelText('Name'), '  app-backends ')
    await user.type(screen.getByLabelText('Server host / IP'), '10.0.0.2')
    await user.type(screen.getByLabelText('Server port'), '8080')
    await user.click(screen.getByRole('button', { name: 'Add server' }))
    await user.type(screen.getAllByLabelText('Server host / IP')[1], 'app.internal')
    await user.type(screen.getAllByLabelText('Server port')[1], '9000')
    await user.click(screen.getAllByLabelText('backup')[1])
    await user.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSaved).toHaveBeenCalled())
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/upstream-pools')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(String(init?.body))).toEqual({
      name: 'app-backends',
      method: 'round_robin',
      max_fails: 1,
      fail_timeout_secs: 10,
      servers: [
        { host: '10.0.0.2', port: 8080, weight: 1, backup: false, down: false },
        { host: 'app.internal', port: 9000, weight: 1, backup: true, down: false },
      ],
    })
    expect(onSaved).toHaveBeenCalledWith(expect.objectContaining({ id: 9 }))
  })

  it('refuses what Angie would refuse, before anything is sent', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    renderForm(null)

    // No name.
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(await screen.findByText('Enter a name for the pool.')).toBeInTheDocument()

    // A server with no host / port.
    await user.type(screen.getByLabelText('Name'), 'apps')
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(
      await screen.findByText(
        'Each backend server needs a host and a valid port (1–65535).',
      ),
    ).toBeInTheDocument()

    // A pool made only of backups has nothing to send traffic to first.
    await user.type(screen.getByLabelText('Server host / IP'), '10.0.0.2')
    await user.type(screen.getByLabelText('Server port'), '8080')
    await user.click(screen.getByLabelText('backup'))
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(
      await screen.findByText('At least one server must not be a backup.'),
    ).toBeInTheDocument()

    // No servers at all.
    await user.click(screen.getByRole('button', { name: 'Remove server' }))
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(
      await screen.findByText('Add at least one backend server.'),
    ).toBeInTheDocument()

    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('edits an existing pool with PUT and puts a name clash on the name field', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(
        {
          error: {
            code: 'upstream_pool_name_taken',
            message: "an upstream pool named 'parked' already exists",
          },
        },
        409,
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    renderForm(apps)

    // Opened unchanged: nothing to save yet.
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
    expect(screen.getAllByLabelText('Server host / IP')).toHaveLength(3)

    const name = screen.getByLabelText('Name')
    await user.clear(name)
    await user.type(name, 'parked')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText("an upstream pool named 'parked' already exists"),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/upstream-pools/3',
      expect.objectContaining({ method: 'PUT' }),
    )
  })
})

describe('deleting an upstream pool', () => {
  it('surfaces the 409 upstream_pool_in_use message verbatim', async () => {
    const user = userEvent.setup()
    const message =
      'upstream pool is used by host(s) #1 (a.example.com); point them elsewhere first'
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        jsonResponse({ error: { code: 'upstream_pool_in_use', message } }, 409),
      )
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <DeleteUpstreamPoolDialog pool={apps} onOpenChange={() => {}} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Delete' }))

    expect(await screen.findByText(message)).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/upstream-pools/3',
      expect.objectContaining({ method: 'DELETE' }),
    )
  })
})

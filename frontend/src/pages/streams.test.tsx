import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Stream } from '@/lib/api'

import { StreamEditorForm, StreamsPage } from './streams'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const sampleStream: Stream = {
  id: 1,
  incoming_port: 5432,
  forward_host: '192.168.1.20',
  forward_port: 5432,
  tcp: true,
  udp: false,
  enabled: true,
  created_at: 1751700000,
  updated_at: 1751700000,
}

/** Route fetches by path so page + dashboard queries both resolve. */
function routedFetch(streams: Stream[], contextActive: boolean) {
  return vi.fn((input: string) => {
    if (input === '/api/streams') {
      return Promise.resolve(jsonResponse({ streams }))
    }
    if (input === '/api/dashboard') {
      return Promise.resolve(
        jsonResponse({
          angie: { up: false, version: null, generation: null, load_time: null, connections: null },
          hosts: [],
          certificates: [],
          streams: { configured: streams.length, enabled: streams.length, context_active: contextActive },
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

afterEach(() => {
  vi.unstubAllGlobals()
})

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
}

describe('streams page', () => {
  it('renders the table from a mocked fetch', async () => {
    vi.stubGlobal('fetch', routedFetch([sampleStream], true))

    render(
      <QueryClientProvider client={makeClient()}>
        <StreamsPage />
      </QueryClientProvider>,
    )

    // Incoming port and forward target render.
    expect(await screen.findByText('5432')).toBeInTheDocument()
    expect(screen.getByText('192.168.1.20:5432')).toBeInTheDocument()
    // Protocol badge and enabled pill.
    expect(screen.getByText('TCP')).toBeInTheDocument()
    expect(screen.getByText('Enabled')).toBeInTheDocument()
    // Context is active → no enable banner.
    expect(screen.queryByText(/stream context is off/i)).not.toBeInTheDocument()
  })

  it('shows the enable banner when the stream context is off', async () => {
    vi.stubGlobal('fetch', routedFetch([sampleStream], false))

    render(
      <QueryClientProvider client={makeClient()}>
        <StreamsPage />
      </QueryClientProvider>,
    )

    expect(
      await screen.findByRole('button', { name: 'Enable streams' }),
    ).toBeInTheDocument()
  })

  it('shows an empty state when there are no streams', async () => {
    vi.stubGlobal('fetch', routedFetch([], true))

    render(
      <QueryClientProvider client={makeClient()}>
        <StreamsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText(/No streams yet/i)).toBeInTheDocument()
  })
})

describe('stream editor', () => {
  it('blocks submit and shows errors when required fields are empty', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <StreamEditorForm stream={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Enter the incoming port (1–65535).'),
    ).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalledWith('/api/streams', expect.anything())
  })

  it('requires at least one protocol', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <StreamEditorForm stream={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.type(screen.getByLabelText('Incoming port'), '5432')
    await user.type(screen.getByLabelText('Forward host'), '192.168.1.20')
    await user.type(screen.getByLabelText('Forward port'), '5432')
    // Turn TCP off (default on), leaving no protocol selected.
    await user.click(screen.getByLabelText('TCP'))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Select at least one of TCP or UDP.'),
    ).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalledWith('/api/streams', expect.anything())
  })

  it('surfaces a 409 port_conflict message on the port field', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string) => {
      if (input === '/api/streams') {
        return Promise.resolve(
          jsonResponse(
            {
              error: {
                code: 'port_conflict',
                message: 'port 5432 (TCP) is already forwarded by stream #2',
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
        <StreamEditorForm stream={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    await user.type(screen.getByLabelText('Incoming port'), '5432')
    await user.type(screen.getByLabelText('Forward host'), '192.168.1.20')
    await user.type(screen.getByLabelText('Forward port'), '5432')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(
        'port 5432 (TCP) is already forwarded by stream #2',
      ),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/streams',
      expect.objectContaining({ method: 'POST' }),
    )
  })
})

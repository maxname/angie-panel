import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import { isValidDomain } from '@/lib/domain'

import { HostEditorForm } from './host-editor-dialog'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

// The form loads the certificate and access-list pickers on mount; answer those
// calls so it can render without hitting the network.
function stubFetch(): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn((input: string) => {
    if (input === '/api/certificates') {
      return Promise.resolve(jsonResponse({ certificates: [] }))
    }
    if (input === '/api/access-lists') {
      return Promise.resolve(jsonResponse({ access_lists: [] }))
    }
    return Promise.reject(new Error(`unexpected fetch ${input}`))
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function renderForm() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <HostEditorForm host={null} onDone={() => {}} />
    </QueryClientProvider>,
  )
}

describe('host editor validation', () => {
  it('accepts FQDNs and rejects junk in isValidDomain', () => {
    expect(isValidDomain('example.com')).toBe(true)
    expect(isValidDomain('*.example.com')).toBe(true)
    expect(isValidDomain('not a domain')).toBe(false)
    expect(isValidDomain('example')).toBe(false)
  })

  it('blocks submit and shows an error when no domains are entered', async () => {
    const user = userEvent.setup()
    const fetchMock = stubFetch()

    renderForm()

    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('Add at least one domain name.'),
    ).toBeInTheDocument()
    // Nothing was submitted to the server (only the certificate picker loaded).
    expect(fetchMock).not.toHaveBeenCalledWith('/api/hosts', expect.anything())
  })

  it('rejects an invalid domain when adding a chip', async () => {
    const user = userEvent.setup()
    stubFetch()
    renderForm()

    await user.type(screen.getByLabelText('Domain names'), 'not valid')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    expect(
      await screen.findByText('That does not look like a valid domain name.'),
    ).toBeInTheDocument()
  })
})

describe('host editor rate limiting', () => {
  async function fillValidBasics(user: ReturnType<typeof userEvent.setup>) {
    await user.type(screen.getByLabelText('Domain names'), 'example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.type(screen.getByLabelText('Forward host'), '10.0.0.5')
  }

  it('requires a limit when rate limiting is enabled', async () => {
    const user = userEvent.setup()
    const fetchMock = stubFetch()
    renderForm()

    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'Rate limit' }))
    await user.click(screen.getByLabelText('Enable rate limiting'))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(
        'Set a request rate or a connection limit before enabling.',
      ),
    ).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalledWith('/api/hosts', expect.anything())
  })

  it('submits the http3 flag from the SSL tab', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/access-lists') {
        return Promise.resolve(jsonResponse({ access_lists: [] }))
      }
      if (input === '/api/hosts' && init?.method === 'POST') {
        return Promise.resolve(jsonResponse({ id: 1 }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderForm()

    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'SSL' }))
    await user.click(screen.getByLabelText('HTTP/3 (QUIC) support'))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.http3).toBe(true)
  })

  it('adds a backend server and submits the upstream pool', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/access-lists') {
        return Promise.resolve(jsonResponse({ access_lists: [] }))
      }
      if (input === '/api/hosts' && init?.method === 'POST') {
        return Promise.resolve(jsonResponse({ id: 1 }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderForm()

    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'Upstreams' }))
    await user.click(screen.getByRole('button', { name: 'Add server' }))
    await user.type(screen.getByLabelText('Server host / IP'), '10.0.0.2')
    await user.type(screen.getByLabelText('Server port'), '8080')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.upstream.servers).toEqual([
      { host: '10.0.0.2', port: 8080, weight: 1, backup: false, down: false },
    ])
  })

  it('blocks an incomplete backend server', async () => {
    const user = userEvent.setup()
    const fetchMock = stubFetch()
    renderForm()

    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'Upstreams' }))
    await user.click(screen.getByRole('button', { name: 'Add server' }))
    // Host left blank → validation error, nothing submitted.
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText(
        'Each backend server needs a host and a valid port (1–65535).',
      ),
    ).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalledWith('/api/hosts', expect.anything())
  })

  it('submits the rate_limit config in the create payload', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/access-lists') {
        return Promise.resolve(jsonResponse({ access_lists: [] }))
      }
      if (input === '/api/hosts' && init?.method === 'POST') {
        return Promise.resolve(jsonResponse({ id: 1 }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderForm()

    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'Rate limit' }))
    await user.click(screen.getByLabelText('Enable rate limiting'))
    await user.type(screen.getByLabelText('Requests / second'), '10')
    await user.type(screen.getByLabelText('Burst'), '20')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    // The POST body carries the rate_limit object.
    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.rate_limit).toEqual({
      enabled: true,
      rps: 10,
      burst: 20,
      nodelay: false,
      conn: 0,
    })
  })
})

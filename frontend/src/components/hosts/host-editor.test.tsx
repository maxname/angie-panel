import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Host, UpstreamPool } from '@/lib/api'
import { isValidDomain } from '@/lib/domain'

import { HostEditorDialog, HostEditorForm } from './host-editor-dialog'

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

  it('focuses the first invalid field on submit', async () => {
    const user = userEvent.setup()
    stubFetch()
    renderForm()

    // Empty submit → the domain entry field takes focus.
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() =>
      expect(screen.getByLabelText('Domain names')).toHaveFocus(),
    )

    // With a domain but no forward host, focus jumps to the forward-host field.
    await user.type(screen.getByLabelText('Domain names'), 'example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() =>
      expect(screen.getByLabelText('Forward host')).toHaveFocus(),
    )
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

describe('host editor unsaved-changes guard', () => {
  function renderDialog(onOpenChange: (open: boolean) => void) {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    })
    return render(
      <QueryClientProvider client={queryClient}>
        <HostEditorDialog open onOpenChange={onOpenChange} host={null} />
      </QueryClientProvider>,
    )
  }

  it('confirms before closing when there are unsaved edits', async () => {
    const user = userEvent.setup()
    stubFetch()
    const onOpenChange = vi.fn()
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false)
    renderDialog(onOpenChange)

    // Dirty the form.
    await user.type(screen.getByLabelText('Forward host'), '10.0.0.9')

    // Cancel while dirty → confirm fires; declining keeps the dialog open.
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(confirmSpy).toHaveBeenCalledTimes(1)
    expect(onOpenChange).not.toHaveBeenCalled()

    // Accepting the confirm closes it.
    confirmSpy.mockReturnValue(true)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(onOpenChange).toHaveBeenCalledWith(false)

    confirmSpy.mockRestore()
  })

  it('closes without confirming when the form is untouched', async () => {
    const user = userEvent.setup()
    stubFetch()
    const onOpenChange = vi.fn()
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)
    renderDialog(onOpenChange)

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(confirmSpy).not.toHaveBeenCalled()
    expect(onOpenChange).toHaveBeenCalledWith(false)

    confirmSpy.mockRestore()
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

  it('defaults Force SSL on for a new host and submits it', async () => {
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
    expect(screen.getByLabelText('Force SSL')).toBeChecked()
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.force_ssl).toBe(true)
  })

  it('submits force_ssl false when Force SSL is switched off', async () => {
    // The toggle is a real opt-out: the generator only emits the :80 -> :443
    // redirect when this is true.
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
    await user.click(screen.getByLabelText('Force SSL'))
    expect(screen.getByLabelText('Force SSL')).not.toBeChecked()
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.force_ssl).toBe(false)
  })

  it('submits the mTLS CA bundle from the SSL tab', async () => {
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

    // No newlines/braces so userEvent types it verbatim.
    const ca = '-----BEGIN CERTIFICATE-----MIIByjExampleCAdata==-----END CERTIFICATE-----'
    await fillValidBasics(user)
    await user.click(screen.getByRole('tab', { name: 'SSL' }))
    await user.type(screen.getByPlaceholderText(/BEGIN CERTIFICATE/), ca)
    // The "optional" toggle only appears once a CA is present.
    await user.click(screen.getByLabelText(/Optional/))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.mtls).toEqual({ ca_pem: ca, optional: true })
  })

  it('omits mTLS when no CA is entered', async () => {
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
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.mtls).toEqual({ ca_pem: null, optional: false })
  })

  it('submits the forward-auth config from the details tab', async () => {
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
    // Forward-auth lives on its own SSO tab.
    await user.click(screen.getByRole('tab', { name: 'SSO' }))
    await user.click(screen.getByLabelText('Enable forward auth'))
    await user.type(
      screen.getByLabelText('Verify endpoint'),
      'http://10.0.0.9:9091/api/verify',
    )
    await user.type(
      screen.getByLabelText('Sign-in URL (optional)'),
      'https://auth.example.com',
    )
    await user.type(
      screen.getByLabelText('Copy identity headers (optional)'),
      'Remote-User, Remote-Groups',
    )
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.forward_auth).toEqual({
      enabled: true,
      verify_url: 'http://10.0.0.9:9091/api/verify',
      sign_in_url: 'https://auth.example.com',
      copy_headers: ['Remote-User', 'Remote-Groups'],
    })
  })

  it('submits custom headers from the headers tab', async () => {
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
    await user.click(screen.getByRole('tab', { name: 'Headers' }))
    await user.click(screen.getByRole('button', { name: 'Add header' }))
    await user.type(screen.getByLabelText('Name'), 'X-Frame-Options')
    await user.type(screen.getByLabelText('Value'), 'SAMEORIGIN')
    // Direction defaults to Response; leave it.
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.custom_headers).toEqual([
      { name: 'X-Frame-Options', value: 'SAMEORIGIN', direction: 'response' },
    ])
  })

  it('submits maintenance mode from the maintenance tab', async () => {
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
    await user.click(screen.getByRole('tab', { name: 'Maintenance' }))
    await user.click(screen.getByLabelText('Enable maintenance mode'))
    await user.type(screen.getByLabelText('Heading'), 'Back soon')
    await user.type(screen.getByLabelText('Message'), 'Upgrading now.')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.maintenance).toEqual({
      enabled: true,
      title: 'Back soon',
      message: 'Upgrading now.',
    })
  })

  it('submits gzip settings from the compression tab', async () => {
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
    await user.click(screen.getByRole('tab', { name: 'Compression' }))
    await user.click(screen.getByLabelText('Enable gzip'))
    await user.type(screen.getByLabelText('Compression level'), '6')
    await user.type(screen.getByLabelText('Minimum length (bytes)'), '256')
    await user.type(screen.getByLabelText('MIME types'), 'text/css, application/json')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    expect(post).toBeTruthy()
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.gzip).toEqual({
      enabled: true,
      comp_level: 6,
      min_length: 256,
      types: ['text/css', 'application/json'],
    })
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

describe('host editor — request a new certificate', () => {
  it('sends new_certificate instead of certificate_id, forcing DNS-01 for a wildcard', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/access-lists') {
        return Promise.resolve(jsonResponse({ access_lists: [] }))
      }
      if (input === '/api/dns-credentials') {
        return Promise.resolve(
          jsonResponse({
            credentials: [
              {
                id: 3,
                provider: 'pdns',
                provider_label: 'PowerDNS',
                name: 'home',
                configured: true,
              },
            ],
          }),
        )
      }
      if (input === '/api/acme/cas') {
        return Promise.resolve(
          jsonResponse({
            default_ca: 'zerossl',
            shared_account: true,
            account_key_path: '/var/lib/angie/acme/angie-panel-account.key',
            cas: [
              {
                id: 'zerossl',
                label: 'ZeroSSL',
                directory: 'https://acme.zerossl.com/v2/DV90',
                staging: false,
                eab: 'required',
                eab_configured: true,
              },
            ],
          }),
        )
      }
      if (input === '/api/hosts' && init?.method === 'POST') {
        return Promise.resolve(jsonResponse({ id: 1 }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderForm()

    await user.type(screen.getByLabelText('Domain names'), '*.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.type(screen.getByLabelText('Forward host'), '10.0.0.5')
    await user.click(screen.getByRole('tab', { name: 'SSL' }))
    await user.click(screen.getByRole('combobox', { name: 'Certificate' }))
    await user.click(
      await screen.findByRole('option', { name: 'Request a new certificate' }),
    )

    // The note names the default CA; the wildcard pins the challenge to DNS-01.
    expect(await screen.findByText(/ZeroSSL, the default/)).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Validation method' })).toBeDisabled()
    await user.click(
      screen.getByRole('combobox', { name: 'How to answer the DNS challenge' }),
    )
    await user.click(await screen.findByRole('option', { name: 'home — PowerDNS' }))
    await user.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() =>
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
        ),
      ).toBe(true),
    )
    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/hosts' && init?.method === 'POST',
    )
    const body = JSON.parse(String((post![1] as RequestInit).body))
    expect(body.certificate_id).toBeNull()
    expect(body.new_certificate).toEqual({ challenge: 'dns', dns_provider: '3' })
  })
})

describe('host editor shared upstream pool', () => {
  const apps: UpstreamPool = {
    id: 3,
    name: 'apps',
    method: 'least_conn',
    max_fails: 1,
    fail_timeout_secs: 10,
    servers: [
      { host: '10.0.0.2', port: 8080, weight: 2, backup: false, down: false },
      { host: '10.0.0.3', port: 8080, weight: 1, backup: true, down: false },
    ],
    created_at: 1751700000,
    updated_at: 1751700000,
  }

  // A host as the API returns it once it proxies to a pool: no target of its own.
  const pooledHost: Host = {
    id: 7,
    domains: ['example.com'],
    forward_scheme: 'http',
    forward_host: '',
    forward_port: 0,
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
    upstream_pool_id: 3,
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

  /** A stateful stand-in for the API: lists the pools, creates new ones (which
   *  then show up in the list) and accepts a host save. */
  function stubPoolApi(pools: UpstreamPool[]) {
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (input === '/api/certificates') {
        return Promise.resolve(jsonResponse({ certificates: [] }))
      }
      if (input === '/api/access-lists') {
        return Promise.resolve(jsonResponse({ access_lists: [] }))
      }
      if (input === '/api/upstream-pools' && method === 'GET') {
        return Promise.resolve(jsonResponse({ upstream_pools: pools }))
      }
      if (input === '/api/upstream-pools' && method === 'POST') {
        const created: UpstreamPool = {
          ...JSON.parse(String(init?.body)),
          id: 9,
          created_at: 1751700000,
          updated_at: 1751700000,
        }
        pools = [...pools, created]
        return Promise.resolve(jsonResponse(created))
      }
      if (input.startsWith('/api/hosts') && method !== 'GET') {
        return Promise.resolve(jsonResponse({ id: 7 }))
      }
      return Promise.reject(new Error(`unexpected fetch ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    return fetchMock
  }

  function renderHostForm(host: Host | null) {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    })
    return render(
      <QueryClientProvider client={queryClient}>
        <HostEditorForm host={host} onDone={() => {}} />
      </QueryClientProvider>,
    )
  }

  function hostSaves(fetchMock: ReturnType<typeof stubPoolApi>) {
    return fetchMock.mock.calls.filter(
      ([url, init]) => url.startsWith('/api/hosts') && init?.method !== undefined,
    )
  }

  it('proxies a new host to a picked pool, with no forward host of its own', async () => {
    const user = userEvent.setup()
    const fetchMock = stubPoolApi([apps])
    renderHostForm(null)

    // A domain and nothing else: no forward host is typed anywhere.
    await user.type(screen.getByLabelText('Domain names'), 'example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    await user.click(screen.getByRole('tab', { name: 'Upstreams' }))
    await user.click(
      screen.getByRole('combobox', { name: 'Backend servers come from' }),
    )
    await user.click(await screen.findByRole('option', { name: 'Pool: apps' }))

    // The pool's servers are shown read-only; the host's own server list and
    // balancing controls are gone, since they would not be used.
    expect(screen.getByText('10.0.0.2:8080')).toBeInTheDocument()
    expect(screen.getByText('weight 2')).toBeInTheDocument()
    expect(screen.getByText('10.0.0.3:8080')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Add server' })).toBeNull()
    expect(screen.queryByLabelText('Balancing method')).toBeNull()

    // Details names the pool where the host:port fields were.
    await user.click(screen.getByRole('tab', { name: 'Details' }))
    expect(screen.getByLabelText('Upstream pool')).toHaveValue('apps')
    expect(screen.queryByLabelText('Forward host')).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() => expect(hostSaves(fetchMock)).toHaveLength(1))
    const body = JSON.parse(String(hostSaves(fetchMock)[0][1]?.body))
    expect(body.upstream_pool_id).toBe(3)
    expect(body.forward_host).toBe('')
    expect(body.forward_port).toBe(0)
    // Balancing and servers are the pool's — the host sends none of its own.
    expect(body).not.toHaveProperty('upstream')
  })

  it('creates a pool from the host editor and selects it, without saving the host', async () => {
    const user = userEvent.setup()
    const fetchMock = stubPoolApi([apps])
    renderHostForm(null)

    await user.type(screen.getByLabelText('Domain names'), 'example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.click(screen.getByRole('tab', { name: 'Upstreams' }))
    await user.click(screen.getByRole('button', { name: 'New pool' }))

    // The pool dialog is modal: its fields and its Save are the reachable ones.
    const dialog = await screen.findByRole('dialog', { name: 'New upstream pool' })
    await user.type(screen.getByLabelText('Name'), 'fresh')
    await user.type(screen.getByLabelText('Server host / IP'), '10.0.0.9')
    await user.type(screen.getByLabelText('Server port'), '9000')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(dialog).not.toBeInTheDocument())
    const created = fetchMock.mock.calls.find(
      ([url, init]) => url === '/api/upstream-pools' && init?.method === 'POST',
    )
    expect(JSON.parse(String(created![1]?.body))).toEqual({
      name: 'fresh',
      method: 'round_robin',
      max_fails: 1,
      fail_timeout_secs: 10,
      servers: [
        { host: '10.0.0.9', port: 9000, weight: 1, backup: false, down: false },
      ],
    })
    // Saving the pool must not have saved the host it was created from.
    expect(hostSaves(fetchMock)).toHaveLength(0)

    // The new pool is already the host's backend — no hunting for it in the list.
    expect(
      await screen.findByRole('combobox', { name: 'Backend servers come from' }),
    ).toHaveTextContent('Pool: fresh')
    expect(screen.getByText('10.0.0.9:9000')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() => expect(hostSaves(fetchMock)).toHaveLength(1))
    const body = JSON.parse(String(hostSaves(fetchMock)[0][1]?.body))
    expect(body.upstream_pool_id).toBe(9)
  })

  it('needs a real target again when a pooled host goes back to its own servers', async () => {
    const user = userEvent.setup()
    const fetchMock = stubPoolApi([apps])
    renderHostForm(pooledHost)

    // Opened on a pool: Details says so.
    expect(await screen.findByDisplayValue('apps')).toBe(
      screen.getByLabelText('Upstream pool'),
    )

    // "Change" leads to the section where the backend is chosen.
    await user.click(screen.getByRole('button', { name: 'Change' }))
    await user.click(
      screen.getByRole('combobox', { name: 'Backend servers come from' }),
    )
    await user.click(
      await screen.findByRole('option', { name: 'This host’s own servers' }),
    )
    expect(screen.getByRole('button', { name: 'Add server' })).toBeInTheDocument()

    // The stored target was empty (port 0); the form offers the usual default
    // port and insists on a host rather than saving a host that goes nowhere.
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(await screen.findByText('Enter the forward host.')).toBeInTheDocument()
    expect(screen.getByLabelText('Port')).toHaveValue(80)
    expect(hostSaves(fetchMock)).toHaveLength(0)

    await user.type(screen.getByLabelText('Forward host'), '10.0.0.5')
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() => expect(hostSaves(fetchMock)).toHaveLength(1))
    const [url, init] = hostSaves(fetchMock)[0]
    expect(url).toBe('/api/hosts/7')
    const body = JSON.parse(String(init?.body))
    expect(body.upstream_pool_id).toBeNull()
    expect(body.forward_host).toBe('10.0.0.5')
    expect(body.forward_port).toBe(80)
  })

  it('says so when the pool a host points at has been deleted', async () => {
    const user = userEvent.setup()
    stubPoolApi([])
    renderHostForm(pooledHost)

    await user.click(screen.getByRole('tab', { name: 'Upstreams' }))
    expect(
      await screen.findByText('This pool no longer exists. Pick another one.'),
    ).toBeInTheDocument()
  })
})

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Cert } from '@/lib/api'

import { CertificatesPage, CertWizardForm } from './certificates'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const unknownCert: Cert = {
  id: 1,
  name: 'no_status',
  domains: ['unknown.example.com'],
  challenge: 'http',
  key_type: 'ecdsa',
  email: null,
  staging: false,
  dns_provider: null,
  ca: 'letsencrypt',
  created_at: 1751700000,
  // Angie status API unreachable → the whole status object is null.
  status: null,
}

const acmeCas = {
  default_ca: 'letsencrypt',
  shared_account: false,
  account_key_path: '/var/lib/angie/acme/angie-panel-account.key',
  cas: [
    {
      id: 'letsencrypt',
      label: "Let's Encrypt",
      directory: 'https://acme-v02.api.letsencrypt.org/directory',
      staging: true,
      eab: 'none',
      eab_configured: false,
    },
    {
      id: 'zerossl',
      label: 'ZeroSSL',
      directory: 'https://acme.zerossl.com/v2/DV90',
      staging: false,
      eab: 'required',
      eab_configured: false,
    },
  ],
}

const validCert: Cert = {
  id: 2,
  name: 'live_site',
  domains: ['example.com', 'www.example.com'],
  challenge: 'dns',
  key_type: 'rsa',
  email: 'admin@example.com',
  staging: true,
  dns_provider: null,
  ca: 'letsencrypt',
  created_at: 1751700000,
  status: { state: 'valid', certificate: 'valid' },
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <CertificatesPage />
    </QueryClientProvider>,
  )
}

function renderWizard() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <CertWizardForm onDone={() => {}} />
    </QueryClientProvider>,
  )
}

describe('certificates page', () => {
  it('renders the table from a mocked fetch, deriving the status pill', async () => {
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve(
        url === '/api/acme/cas'
          ? jsonResponse(acmeCas)
          : jsonResponse({ certificates: [unknownCert, validCert] }),
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    renderPage()

    // Domains are the primary identifier, rendered as badges.
    expect(await screen.findByText('unknown.example.com')).toBeInTheDocument()
    expect(screen.getByText('example.com')).toBeInTheDocument()
    // Status pills: null → "Unknown", certificate "valid" → "Issued".
    expect(screen.getByText('Unknown')).toBeInTheDocument()
    expect(screen.getByText('Issued')).toBeInTheDocument()
    // Staging certificate gets the amber STAGING badge.
    expect(screen.getByText('STAGING')).toBeInTheDocument()

    expect(fetchMock).toHaveBeenCalledWith('/api/certificates', expect.anything())
    // Each row names its CA.
    expect(await screen.findAllByText("Let's Encrypt")).toHaveLength(2)
  })
})

describe('certificate wizard', () => {
  it('forces DNS-01 and disables the other challenges for wildcard domains', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', vi.fn())

    renderWizard()

    // Before a wildcard: HTTP-01 is selected and enabled.
    const httpRadio = screen.getByRole('radio', { name: /HTTP-01/ })
    expect(httpRadio).toBeChecked()
    expect(httpRadio).toBeEnabled()

    // Add a wildcard domain.
    await user.type(screen.getByLabelText('Domains'), '*.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    // DNS-01 is now forced, the others disabled.
    expect(screen.getByRole('radio', { name: /DNS-01/ })).toBeChecked()
    expect(screen.getByRole('radio', { name: /HTTP-01/ })).toBeDisabled()
    expect(screen.getByRole('radio', { name: /TLS-ALPN-01/ })).toBeDisabled()
    expect(
      screen.getByText('Wildcard domains require DNS-01.'),
    ).toBeInTheDocument()
  })

  it('offers the DNS-provider method with a profile picker and warns when unconfigured', async () => {
    const user = userEvent.setup()
    // /api/dns-credentials → one profile, not yet configured.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse({
          credentials: [
            {
              id: 5,
              provider: 'cloudflare',
              provider_label: 'Cloudflare',
              name: 'CF work',
              configured: false,
            },
          ],
        }),
      ),
    )
    renderWizard()

    await user.type(screen.getByLabelText('Domains'), '*.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    // Self-answer is the default; the provider option is offered.
    const self = screen.getByRole('radio', { name: /Angie answers/ })
    const provider = screen.getByRole('radio', { name: /DNS provider API/ })
    expect(self).toBeChecked()

    // Choosing the provider reveals the profile picker (defaults to the first
    // profile), and an unconfigured one surfaces the setup hint (by profile name).
    await user.click(provider)
    expect(provider).toBeChecked()
    expect(await screen.findByText(/CF work.*has no credentials/i)).toBeInTheDocument()
  })

  it('edits an existing certificate — prefills the fields and PUTs the update', async () => {
    const user = userEvent.setup()
    const editCert: Cert = {
      id: 7,
      name: 'live_site',
      domains: ['example.com', 'www.example.com'],
      challenge: 'http',
      key_type: 'ecdsa',
      email: 'admin@example.com',
      staging: false,
      dns_provider: null,
      ca: 'letsencrypt',
      created_at: 1751700000,
      status: null,
    }
    const fetchMock = vi.fn((url: string, init?: RequestInit) => {
      if (url === '/api/dns-credentials') {
        return Promise.resolve(jsonResponse({ credentials: [] }))
      }
      if (url === '/api/acme/cas') {
        return Promise.resolve(jsonResponse(acmeCas))
      }
      if (url === '/api/certificates/7' && init?.method === 'PUT') {
        // Echo back the edited cert (still http → wizard just closes).
        return Promise.resolve(
          jsonResponse({ ...editCert, name: 'live_site_v2' }),
        )
      }
      return Promise.reject(new Error(`unexpected ${init?.method ?? 'GET'} ${url}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    const onDone = vi.fn()
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <CertWizardForm cert={editCert} onDone={onDone} />
      </QueryClientProvider>,
    )

    // Existing values are prefilled, and the re-issue note is shown.
    const nameInput = screen.getByLabelText('Name') as HTMLInputElement
    expect(nameInput.value).toBe('live_site')
    expect(screen.getByText('example.com')).toBeInTheDocument()
    expect(screen.getByText(/re-issue the certificate/i)).toBeInTheDocument()

    // Rename and save → PUT to the cert's id, then onDone.
    await user.clear(nameInput)
    await user.type(nameInput, 'live_site_v2')
    await user.click(screen.getByRole('button', { name: 'Save changes' }))

    await vi.waitFor(() => expect(onDone).toHaveBeenCalled())
    const putCall = fetchMock.mock.calls.find(
      ([, init]) => (init as RequestInit | undefined)?.method === 'PUT',
    )
    expect(putCall?.[0]).toBe('/api/certificates/7')
    expect(JSON.parse((putCall?.[1] as RequestInit).body as string)).toMatchObject({
      name: 'live_site_v2',
      domains: ['example.com', 'www.example.com'],
      challenge: 'http',
    })
  })

  it('picks a CA — warns when it lacks EAB, drops staging it lacks, and sends it', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn((url: string, init?: RequestInit) => {
      if (url === '/api/dns-credentials') {
        return Promise.resolve(jsonResponse({ credentials: [] }))
      }
      if (url === '/api/acme/cas') {
        return Promise.resolve(jsonResponse(acmeCas))
      }
      if (url === '/api/certificates' && init?.method === 'POST') {
        return Promise.resolve(jsonResponse({ ...unknownCert, ca: 'zerossl' }))
      }
      return Promise.reject(new Error(`unexpected ${init?.method ?? 'GET'} ${url}`))
    })
    vi.stubGlobal('fetch', fetchMock)
    renderWizard()

    await user.type(screen.getByLabelText('Domains'), 'shop.example.com')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    // Staging is on offer for Let's Encrypt (the default)…
    const staging = screen.getByRole('switch', { name: /staging/i })
    await vi.waitFor(() => expect(staging).toBeEnabled())
    await user.click(staging)

    // …but not for ZeroSSL, which also needs EAB credentials first.
    await user.click(screen.getByRole('combobox', { name: 'Certificate authority' }))
    await user.click(await screen.findByRole('option', { name: 'ZeroSSL' }))
    expect(screen.getByText(/ZeroSSL requires EAB credentials/)).toBeInTheDocument()
    expect(staging).toBeDisabled()
    expect(staging).not.toBeChecked()

    await user.click(screen.getByRole('button', { name: 'Create certificate' }))
    await vi.waitFor(() =>
      expect(
        fetchMock.mock.calls.some(([, init]) => init?.method === 'POST'),
      ).toBe(true),
    )
    const post = fetchMock.mock.calls.find(([, init]) => init?.method === 'POST')
    expect(JSON.parse((post?.[1] as RequestInit).body as string)).toMatchObject({
      ca: 'zerossl',
      staging: false,
    })
  })
})

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
  created_at: 1751700000,
  // Angie status API unreachable → the whole status object is null.
  status: null,
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
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        jsonResponse({ certificates: [unknownCert, validCert] }),
      )
    vi.stubGlobal('fetch', fetchMock)

    renderPage()

    // Names render in a mono cell.
    expect(await screen.findByText('no_status')).toBeInTheDocument()
    expect(screen.getByText('live_site')).toBeInTheDocument()
    // Domains render as badges.
    expect(screen.getByText('unknown.example.com')).toBeInTheDocument()
    expect(screen.getByText('example.com')).toBeInTheDocument()
    // Status pills: null → "Unknown", certificate "valid" → "Issued".
    expect(screen.getByText('Unknown')).toBeInTheDocument()
    expect(screen.getByText('Issued')).toBeInTheDocument()
    // Staging certificate gets the amber STAGING badge.
    expect(screen.getByText('STAGING')).toBeInTheDocument()

    expect(fetchMock).toHaveBeenCalledWith('/api/certificates', expect.anything())
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
})

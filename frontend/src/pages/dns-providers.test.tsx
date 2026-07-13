import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { DnsProviderInfo } from '@/lib/api'

import { DnsProvidersPage } from './dns-providers'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const providers: DnsProviderInfo[] = [
  {
    id: 'cloudflare',
    label: 'Cloudflare',
    fields: [{ env: 'CF_Token', label: 'API token' }],
    configured: true,
  },
  {
    id: 'route53',
    label: 'AWS Route 53',
    fields: [
      { env: 'AWS_ACCESS_KEY_ID', label: 'Access key ID' },
      { env: 'AWS_SECRET_ACCESS_KEY', label: 'Secret access key' },
    ],
    configured: false,
  },
]

beforeAll(async () => {
  await i18n.changeLanguage('en')
})
afterEach(() => vi.unstubAllGlobals())

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={client}>
      <DnsProvidersPage />
    </QueryClientProvider>,
  )
}

describe('DNS providers page', () => {
  it('lists every provider with its configured status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ providers })))
    renderPage()

    expect(await screen.findByText('Cloudflare')).toBeInTheDocument()
    expect(screen.getByText('AWS Route 53')).toBeInTheDocument()
    // One configured, one not — both statuses render; summary counts the config.
    expect(screen.getByText('Configured')).toBeInTheDocument()
    expect(screen.getByText('Not configured')).toBeInTheDocument()
    expect(screen.getByText('Configured providers: 1')).toBeInTheDocument()
  })

  it('opens the editor with the provider’s dynamic credential fields', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ providers })))
    renderPage()

    // The unconfigured provider shows "Configure"; open its editor.
    const row = (await screen.findByText('AWS Route 53')).closest('tr')!
    await user.click(within(row).getByRole('button', { name: 'Configure' }))

    // Dialog titled for the provider, with BOTH of its credential fields.
    expect(
      await screen.findByRole('heading', { name: /AWS Route 53 credentials/i }),
    ).toBeInTheDocument()
    expect(screen.getByLabelText('Access key ID')).toBeInTheDocument()
    expect(screen.getByLabelText('Secret access key')).toBeInTheDocument()
  })
})

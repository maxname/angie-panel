import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'

import { DnsProvidersPage } from './dns-providers'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const TYPES = [
  { id: 'cloudflare', label: 'Cloudflare', fields: [{ env: 'CF_Token', label: 'API token' }] },
  {
    id: 'route53',
    label: 'AWS Route 53',
    fields: [
      { env: 'AWS_ACCESS_KEY_ID', label: 'Access key ID' },
      { env: 'AWS_SECRET_ACCESS_KEY', label: 'Secret access key' },
    ],
  },
]

/** Route the two GETs the page issues. */
function routedFetch(profiles: unknown[]) {
  return vi.fn((input: string) => {
    if (input === '/api/dns-credentials') {
      return Promise.resolve(jsonResponse({ credentials: profiles }))
    }
    if (input === '/api/dns-providers') {
      return Promise.resolve(jsonResponse({ providers: TYPES }))
    }
    return Promise.reject(new Error(`unexpected fetch ${input}`))
  })
}

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
  it('lists credential profiles — two of the same provider coexist', async () => {
    vi.stubGlobal(
      'fetch',
      routedFetch([
        { id: 1, provider: 'cloudflare', provider_label: 'Cloudflare', name: 'CF personal', configured: true },
        { id: 2, provider: 'cloudflare', provider_label: 'Cloudflare', name: 'CF work', configured: true },
      ]),
    )
    renderPage()

    expect(await screen.findByText('CF personal')).toBeInTheDocument()
    expect(screen.getByText('CF work')).toBeInTheDocument()
    // Both are the same provider type — the whole point of profiles.
    expect(screen.getAllByText('Cloudflare')).toHaveLength(2)
  })

  it('empty state, then the Add dialog picks a type and shows its fields', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', routedFetch([]))
    renderPage()

    expect(await screen.findByText(/No provider profiles yet/i)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Add profile' }))
    // Dialog: create title + a name field + the default type's credential field.
    expect(
      await screen.findByRole('heading', { name: /Add DNS provider profile/i }),
    ).toBeInTheDocument()
    expect(screen.getByLabelText('Profile name')).toBeInTheDocument()
    // Default type is Cloudflare → its single API-token field renders.
    const dialog = screen.getByRole('dialog')
    expect(within(dialog).getByLabelText('API token')).toBeInTheDocument()
  })
})

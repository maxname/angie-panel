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

// The SSL tab loads the certificate picker on mount; answer that call so the
// form can render without hitting the network.
function stubFetch(): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn((input: string) => {
    if (input === '/api/certificates') {
      return Promise.resolve(jsonResponse({ certificates: [] }))
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

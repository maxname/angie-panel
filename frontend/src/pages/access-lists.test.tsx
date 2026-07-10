import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { AccessList } from '@/lib/api'

import {
  AccessListEditorForm,
  AccessListsPage,
  DeleteAccessListDialog,
} from './access-lists'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const gatedList: AccessList = {
  id: 1,
  name: 'internal_only',
  satisfy: 'all',
  pass_auth: true,
  users: [
    { username: 'alice', has_password: true },
    { username: 'bob', has_password: true },
  ],
  clients: [{ directive: 'allow', address: '192.168.0.0/16' }],
  created_at: 1751700000,
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

describe('access lists page', () => {
  it('renders the table from a mocked fetch with the summary and badges', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(jsonResponse({ access_lists: [gatedList] }))
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <AccessListsPage />
      </QueryClientProvider>,
    )

    // Name renders in a mono cell.
    expect(await screen.findByText('internal_only')).toBeInTheDocument()
    // Summary combines user and IP-rule counts.
    expect(screen.getByText('2 users · 1 IP rules')).toBeInTheDocument()
    // satisfy "all" → "All" badge.
    expect(screen.getByText('All')).toBeInTheDocument()
    // pass_auth on → a "Pass auth" badge (plus the column header of the same text).
    expect(screen.getAllByText('Pass auth').length).toBeGreaterThanOrEqual(2)

    expect(fetchMock).toHaveBeenCalledWith('/api/access-lists', expect.anything())
  })
})

describe('access list editor', () => {
  it('appends a basic-auth user row and an IP-rule row on demand', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', vi.fn())

    render(
      <QueryClientProvider client={makeClient()}>
        <AccessListEditorForm list={null} onDone={() => {}} />
      </QueryClientProvider>,
    )

    // Starts with no users and no rules.
    expect(screen.queryByLabelText('Username')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Address')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Add user' }))
    expect(screen.getByLabelText('Username')).toBeInTheDocument()
    expect(screen.getByLabelText('Password')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Add rule' }))
    expect(screen.getByLabelText('Address')).toBeInTheDocument()
  })
})

describe('deleting an access list', () => {
  it('surfaces the 409 access_list_in_use message verbatim', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(
        {
          error: {
            code: 'access_list_in_use',
            message: 'Still in use by hosts: example.com',
          },
        },
        409,
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <DeleteAccessListDialog list={gatedList} onOpenChange={() => {}} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Delete' }))

    expect(
      await screen.findByText('Still in use by hosts: example.com'),
    ).toBeInTheDocument()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/access-lists/1',
      expect.objectContaining({ method: 'DELETE' }),
    )
  })
})

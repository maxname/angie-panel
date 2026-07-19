import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { ApiToken } from '@/lib/api'

import { TokensPage } from './tokens'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const ciToken: ApiToken = {
  id: 1,
  name: 'ci-deploy',
  prefix: 'ap_3f9a2b1c…',
  owner: 'admin@example.com',
  is_local: false,
  created_at: 1751700000,
  last_used_at: 1751800000,
  expires_at: null,
}

const localToken: ApiToken = {
  id: 2,
  name: 'apctl',
  prefix: 'ap_aabbccdd…',
  owner: null,
  is_local: true,
  created_at: 1751700000,
  last_used_at: null,
  expires_at: null,
}

function routedFetch(tokens: ApiToken[]) {
  return vi.fn((input: string) => {
    if (input === '/api/tokens') {
      return Promise.resolve(jsonResponse({ tokens }))
    }
    return Promise.reject(new Error(`unexpected fetch ${input}`))
  })
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})
afterEach(() => vi.unstubAllGlobals())

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
}

function renderPage() {
  render(
    <QueryClientProvider client={makeClient()}>
      <TokensPage />
    </QueryClientProvider>,
  )
}

describe('tokens page', () => {
  it('lists tokens without ever showing a usable secret', async () => {
    vi.stubGlobal('fetch', routedFetch([ciToken, localToken]))
    renderPage()

    expect(await screen.findByText('ci-deploy')).toBeInTheDocument()
    expect(screen.getByText('ap_3f9a2b1c…')).toBeInTheDocument()
    // The machine-local token is labelled and attributed to no account.
    expect(screen.getByText('local')).toBeInTheDocument()
    expect(screen.getByText('this machine')).toBeInTheDocument()
  })

  it('does not offer to revoke the local token, which is rotated on disk', async () => {
    vi.stubGlobal('fetch', routedFetch([ciToken, localToken]))
    renderPage()

    await screen.findByText('ci-deploy')
    const revokeButtons = screen.getAllByRole('button', { name: 'Revoke' })
    expect(revokeButtons).toHaveLength(2)
    // Row order matches the fixture: ci-deploy first, the local token second.
    expect(revokeButtons[0]).toBeEnabled()
    expect(revokeButtons[1]).toBeDisabled()
  })

  it('asks for confirmation before revoking', async () => {
    const user = userEvent.setup()
    const deletes: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const method = init?.method ?? 'GET'
        if (input === '/api/tokens' && method === 'GET') {
          return Promise.resolve(jsonResponse({ tokens: [ciToken] }))
        }
        if (input === '/api/tokens/1' && method === 'DELETE') {
          deletes.push(input)
          return Promise.resolve(jsonResponse({ ok: true }))
        }
        return Promise.reject(new Error(`unexpected fetch ${method} ${input}`))
      }),
    )
    renderPage()

    await screen.findByText('ci-deploy')
    await user.click(screen.getByRole('button', { name: 'Revoke' }))

    expect(await screen.findByText('Revoke token?')).toBeInTheDocument()
    expect(deletes).toHaveLength(0)

    const confirm = screen.getAllByRole('button', { name: 'Revoke' })
    await user.click(confirm[confirm.length - 1])
    await waitFor(() => expect(deletes).toEqual(['/api/tokens/1']))
  })

  /** The whole point of the flow: the secret exists in the UI exactly once. */
  it('shows the new secret once, and not after the dialog is dismissed', async () => {
    const user = userEvent.setup()
    const secret = `ap_${'a1b2c3d4'.repeat(8)}`
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const method = init?.method ?? 'GET'
        if (input === '/api/tokens' && method === 'GET') {
          return Promise.resolve(jsonResponse({ tokens: [] }))
        }
        if (input === '/api/tokens' && method === 'POST') {
          return Promise.resolve(jsonResponse({ id: 7, name: 'ci-deploy', secret }))
        }
        return Promise.reject(new Error(`unexpected fetch ${method} ${input}`))
      }),
    )
    renderPage()

    await screen.findByText(
      'No tokens yet. Create one to use apctl from another machine.',
    )
    await user.click(screen.getByRole('button', { name: 'New token' }))
    await user.type(screen.getByLabelText('Name'), 'ci-deploy')
    await user.click(screen.getByRole('button', { name: 'Create token' }))

    // Shown once, with the warning that it will not be shown again.
    const field = await screen.findByLabelText<HTMLInputElement>('Token value')
    expect(field.value).toBe(secret)
    expect(
      screen.getByText('This is the only time it is shown'),
    ).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Done' }))
    await waitFor(() =>
      expect(screen.queryByLabelText('Token value')).not.toBeInTheDocument(),
    )
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
  })
})

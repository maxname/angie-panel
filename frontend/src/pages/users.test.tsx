import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { User } from '@/lib/api'

import { UsersPage } from './users'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const admin: User = {
  id: 1,
  email: 'admin@example.com',
  role: 'admin',
  created_at: 1751700000,
}
const viewer: User = {
  id: 2,
  email: 'viewer@example.com',
  role: 'viewer',
  created_at: 1751700000,
}

function routedFetch(users: User[]) {
  return vi.fn((input: string) => {
    if (input === '/api/users') {
      return Promise.resolve(jsonResponse({ users }))
    }
    if (input === '/api/auth/me') {
      return Promise.resolve(jsonResponse({ email: 'admin@example.com', role: 'admin' }))
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

describe('users page', () => {
  it('lists users with their roles and marks the current user', async () => {
    vi.stubGlobal('fetch', routedFetch([admin, viewer]))

    render(
      <QueryClientProvider client={makeClient()}>
        <UsersPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('admin@example.com')).toBeInTheDocument()
    expect(screen.getByText('viewer@example.com')).toBeInTheDocument()
    // The signed-in admin is marked "(you)".
    expect(screen.getByText('(you)')).toBeInTheDocument()
  })

  it('opens the create dialog', async () => {
    const user = userEvent.setup()
    vi.stubGlobal('fetch', routedFetch([admin]))

    render(
      <QueryClientProvider client={makeClient()}>
        <UsersPage />
      </QueryClientProvider>,
    )

    await screen.findByText('admin@example.com')
    await user.click(screen.getByRole('button', { name: 'Add user' }))

    expect(
      await screen.findByRole('button', { name: 'Create user' }),
    ).toBeInTheDocument()
  })
})

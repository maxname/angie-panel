import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
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

  it('asks for confirmation before deleting a user', async () => {
    const user = userEvent.setup()
    const deletes: string[] = []
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (input === '/api/users' && method === 'GET') {
        return Promise.resolve(jsonResponse({ users: [admin, viewer] }))
      }
      if (input === '/api/auth/me') {
        return Promise.resolve(jsonResponse({ email: 'admin@example.com', role: 'admin' }))
      }
      if (input === '/api/users/2' && method === 'DELETE') {
        deletes.push(input)
        return Promise.resolve(jsonResponse({ ok: true }))
      }
      return Promise.reject(new Error(`unexpected fetch ${method} ${input}`))
    })
    vi.stubGlobal('fetch', fetchMock)

    render(
      <QueryClientProvider client={makeClient()}>
        <UsersPage />
      </QueryClientProvider>,
    )

    await screen.findByText('viewer@example.com')
    // Open the viewer row's action menu (the second one; the admin is self).
    const menus = screen.getAllByRole('button', { name: 'Actions' })
    await user.click(menus[menus.length - 1])
    await user.click(await screen.findByText('Delete'))

    // A confirmation dialog appears and nothing is deleted yet.
    expect(await screen.findByText('Delete user?')).toBeInTheDocument()
    expect(deletes).toHaveLength(0)

    // Confirm → the delete request fires.
    const confirmButtons = screen.getAllByRole('button', { name: 'Delete' })
    await user.click(confirmButtons[confirmButtons.length - 1])
    await waitFor(() => expect(deletes).toEqual(['/api/users/2']))
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

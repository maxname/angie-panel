import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { AuditEntry } from '@/lib/api'

import { AuditLogPage } from './audit'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const entries: AuditEntry[] = [
  {
    id: 2,
    user_email: 'admin@example.com',
    method: 'POST',
    path: '/api/hosts',
    status: 200,
    created_at: 1751700000,
  },
  {
    id: 1,
    user_email: null,
    method: 'POST',
    path: '/api/auth/login',
    status: 401,
    created_at: 1751600000,
  },
]

beforeAll(async () => {
  await i18n.changeLanguage('en')
})
afterEach(() => vi.unstubAllGlobals())

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <AuditLogPage />
    </QueryClientProvider>,
  )
}

describe('audit log page', () => {
  it('renders entries with derived action labels and status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ entries })))
    renderPage()

    // Derived target label + the raw path + the actor.
    expect(await screen.findByText('Proxy host')).toBeInTheDocument()
    expect(screen.getByText('/api/hosts')).toBeInTheDocument()
    expect(screen.getByText('admin@example.com')).toBeInTheDocument()
    // A failed login by an unauthenticated caller.
    expect(screen.getByText('Sign in')).toBeInTheDocument()
    expect(screen.getByText('anonymous')).toBeInTheDocument()
    expect(screen.getByText('401')).toBeInTheDocument()
  })

  it('shows an empty state', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ entries: [] })))
    renderPage()
    expect(await screen.findByText(/No activity recorded/i)).toBeInTheDocument()
  })
})

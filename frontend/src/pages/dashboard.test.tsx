import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { render, screen } from '@testing-library/react'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import type { Dashboard } from '@/lib/api'

import { DashboardPage } from './dashboard'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

/** Route every fetch: /api/dashboard returns `dashboard`, configtest 404s. */
function stubFetch(dashboard: Dashboard) {
  const fetchMock = vi.fn((input: string) => {
    if (input === '/api/dashboard') {
      return Promise.resolve(jsonResponse(dashboard))
    }
    if (input === '/api/system/configtest') {
      // No configtest has ever been run — the section shows its "never" note.
      return Promise.resolve(
        jsonResponse({ error: { code: 'not_found', message: 'none' } }, 404),
      )
    }
    return Promise.reject(new Error(`unexpected fetch ${input}`))
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

const angieUp: Dashboard = {
  angie: {
    up: true,
    version: '1.8.1',
    generation: 12,
    load_time: '2026-07-10T10:00:00Z',
    connections: { accepted: 5000, active: 7, idle: 3, dropped: 0 },
  },
  hosts: [
    {
      id: 1,
      domains: ['app.example.com'],
      enabled: true,
      forward: 'http://192.168.1.10:3000',
      certificate_id: 10,
      https_active: true,
      zone: {
        requests: { total: 1234, processing: 1, discarded: 0 },
        responses: { '200': 1200, '301': 4, '404': 3, '500': 1, total: 1208 },
        data: { received: 9000, sent: 45000 },
      },
      upstream: { peers_up: 2, peers_down: 0, fails: 0 },
    },
  ],
  certificates: [
    {
      id: 10,
      name: 'shop_cert',
      domains: ['shop.example.com'],
      challenge: 'http',
      staging: false,
      status: { state: 'valid', certificate: 'valid' },
    },
    {
      id: 11,
      name: 'blog_cert',
      domains: ['blog.example.com'],
      challenge: 'dns',
      staging: false,
      status: { state: 'pending', certificate: 'pending' },
    },
  ],
  drift: { detected: true, foreign_files: ['legacy.conf'] },
  pending_changes: false,
  alerts: [
    {
      severity: 'warning',
      code: 'drift',
      message: 'A managed config file was edited on disk; re-apply to restore',
    },
  ],
}

const angieDown: Dashboard = {
  angie: {
    up: false,
    version: null,
    generation: null,
    load_time: null,
    connections: null,
  },
  hosts: [],
  certificates: [],
  drift: { detected: false, foreign_files: [] },
  pending_changes: false,
  alerts: [
    {
      severity: 'error',
      code: 'angie_down',
      message: 'Angie status API is unreachable',
    },
  ],
}

beforeAll(async () => {
  await i18n.changeLanguage('en')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

// TanStack Router's <Link> needs a router in context; mount the page inside a
// minimal memory router that also declares the routes the dashboard links to.
function renderDashboard() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const rootRoute = createRootRoute()
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
    component: () => (
      <QueryClientProvider client={queryClient}>
        <DashboardPage />
      </QueryClientProvider>
    ),
  })
  const stub = (path: string) =>
    createRoute({ getParentRoute: () => rootRoute, path, component: () => null })
  const routeTree = rootRoute.addChildren([
    indexRoute,
    stub('/apply'),
    stub('/certificates'),
    stub('/hosts'),
  ])
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  return render(<RouterProvider router={router} />)
}

describe('dashboard page', () => {
  it('renders angie-up state with a host row, cert pills, and the drift alert', async () => {
    stubFetch(angieUp)
    renderDashboard()

    // Angie card: running pill + connection tiles.
    expect(await screen.findByText('Running')).toBeInTheDocument()
    expect(screen.getByText('5,000')).toBeInTheDocument() // accepted, formatted

    // Host row: domain badge, forward target, HTTPS badge, requests, responses.
    expect(screen.getByText('app.example.com')).toBeInTheDocument()
    expect(screen.getByText('http://192.168.1.10:3000')).toBeInTheDocument()
    expect(screen.getAllByText('HTTPS').length).toBeGreaterThan(0)
    expect(screen.getByText('1,234')).toBeInTheDocument()
    // Response codes bucketed by first digit.
    expect(
      screen.getByText((content) => content.includes('2xx: 1,200')),
    ).toBeInTheDocument()
    expect(
      screen.getByText((content) => content.includes('4xx: 3')),
    ).toBeInTheDocument()
    // Upstream summary.
    expect(screen.getByText('2 up / 0 down')).toBeInTheDocument()

    // Cert pills: valid → Issued, pending → Pending.
    expect(screen.getByText('Issued')).toBeInTheDocument()
    expect(screen.getByText('Pending')).toBeInTheDocument()
    // The pending cert shows the auto-HTTPS reassurance; the issued one does not.
    expect(
      screen.getByText('HTTPS activates automatically once issued.'),
    ).toBeInTheDocument()

    // Drift alert renders with its message and an Apply link.
    expect(
      screen.getByText((content) => content.includes('A managed config file')),
    ).toBeInTheDocument()
    expect(screen.getByText('Re-apply to restore')).toBeInTheDocument()
    // The drift alert also lists the unmanaged file it found.
    expect(screen.getByText('legacy.conf')).toBeInTheDocument()
  })

  it('shows the unreachable note when angie is down', async () => {
    stubFetch(angieDown)
    renderDashboard()

    expect(await screen.findByText('Unreachable')).toBeInTheDocument()
    expect(
      screen.getByText('Angie status API unreachable — is Angie running?'),
    ).toBeInTheDocument()
    // Connection tiles are hidden when Angie is unreachable.
    expect(screen.queryByText('Accepted')).not.toBeInTheDocument()
  })
})

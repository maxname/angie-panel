import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from '@tanstack/react-router'

import { AppShell } from '@/components/layout/app-shell'
import { RouterError, RouterPending } from '@/components/router-fallbacks'
import { api } from '@/lib/api'
// Login and setup stay eager: they are the first paint for anyone without a
// session, and they are small. Every page behind the auth guard is split out,
// so opening the dashboard doesn't also download and parse the audit log, the
// settings page and twelve others. (The SBC only serves these bytes — it's the
// admin's browser, possibly on a phone over a VPN, that pays for them.)
import { LoginPage } from '@/pages/login'
import { SetupPage } from '@/pages/setup'

const rootRoute = createRootRoute({
  component: Outlet,
})

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/setup',
  beforeLoad: async () => {
    const state = await api.getAuthState()
    if (!state.setup_required) {
      throw redirect({ to: state.authenticated ? '/' : '/login' })
    }
  },
  component: SetupPage,
})

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  beforeLoad: async () => {
    const state = await api.getAuthState()
    if (state.setup_required) {
      throw redirect({ to: '/setup' })
    }
    if (state.authenticated) {
      throw redirect({ to: '/' })
    }
  },
  component: LoginPage,
})

// Pathless layout route guarding everything that requires an authenticated session.
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'app',
  beforeLoad: async () => {
    const state = await api.getAuthState()
    if (state.setup_required) {
      throw redirect({ to: '/setup' })
    }
    if (!state.authenticated) {
      throw redirect({ to: '/login' })
    }
  },
  component: AppShell,
})

const dashboardRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/',
  component: lazyRouteComponent(() => import('@/pages/dashboard'), 'DashboardPage'),
})

const hostsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/hosts',
  component: lazyRouteComponent(() => import('@/pages/hosts'), 'HostsPage'),
})

const redirectHostsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/redirect-hosts',
  component: lazyRouteComponent(() => import('@/pages/redirect-hosts'), 'RedirectHostsPage'),
})

const deadHostsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/dead-hosts',
  component: lazyRouteComponent(() => import('@/pages/dead-hosts'), 'DeadHostsPage'),
})

const streamsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/streams',
  component: lazyRouteComponent(() => import('@/pages/streams'), 'StreamsPage'),
})

const sniRoutersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/sni-routers',
  component: lazyRouteComponent(() => import('@/pages/sni-routers'), 'SniRoutersPage'),
})

const certificatesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/certificates',
  component: lazyRouteComponent(() => import('@/pages/certificates'), 'CertificatesPage'),
})

const dnsProvidersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/dns-providers',
  component: lazyRouteComponent(() => import('@/pages/dns-providers'), 'DnsProvidersPage'),
})

const accessListsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/access-lists',
  component: lazyRouteComponent(() => import('@/pages/access-lists'), 'AccessListsPage'),
})

const applyRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/apply',
  component: lazyRouteComponent(() => import('@/pages/apply'), 'ApplyPage'),
})

const blocklistRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/blocklist',
  component: lazyRouteComponent(() => import('@/pages/blocklist'), 'BlocklistPage'),
})

const usersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/users',
  component: lazyRouteComponent(() => import('@/pages/users'), 'UsersPage'),
})

const tokensRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/tokens',
  component: lazyRouteComponent(() => import('@/pages/tokens'), 'TokensPage'),
})

const auditRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/audit',
  component: lazyRouteComponent(() => import('@/pages/audit'), 'AuditLogPage'),
})

const settingsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/settings',
  component: lazyRouteComponent(() => import('@/pages/settings'), 'SettingsPage'),
})

const routeTree = rootRoute.addChildren([
  setupRoute,
  loginRoute,
  appRoute.addChildren([
    dashboardRoute,
    hostsRoute,
    redirectHostsRoute,
    deadHostsRoute,
    streamsRoute,
    sniRoutersRoute,
    certificatesRoute,
    dnsProvidersRoute,
    accessListsRoute,
    blocklistRoute,
    applyRoute,
    usersRoute,
    tokensRoute,
    auditRoute,
    settingsRoute,
  ]),
])

export const router = createRouter({
  routeTree,
  defaultPendingComponent: RouterPending,
  defaultErrorComponent: RouterError,
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from '@tanstack/react-router'

import { AppShell } from '@/components/layout/app-shell'
import { RouterError, RouterPending } from '@/components/router-fallbacks'
import { api } from '@/lib/api'
import { AccessListsPage } from '@/pages/access-lists'
import { ApplyPage } from '@/pages/apply'
import { CertificatesPage } from '@/pages/certificates'
import { DashboardPage } from '@/pages/dashboard'
import { HostsPage } from '@/pages/hosts'
import { LoginPage } from '@/pages/login'
import { SettingsPage } from '@/pages/settings'
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
  component: DashboardPage,
})

const hostsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/hosts',
  component: HostsPage,
})

const certificatesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/certificates',
  component: CertificatesPage,
})

const accessListsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/access-lists',
  component: AccessListsPage,
})

const applyRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/apply',
  component: ApplyPage,
})

const settingsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/settings',
  component: SettingsPage,
})

const routeTree = rootRoute.addChildren([
  setupRoute,
  loginRoute,
  appRoute.addChildren([
    dashboardRoute,
    hostsRoute,
    certificatesRoute,
    accessListsRoute,
    applyRoute,
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

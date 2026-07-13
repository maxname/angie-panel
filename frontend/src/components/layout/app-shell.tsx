import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, Outlet, useRouter } from '@tanstack/react-router'
import {
  Cloud,
  CornerUpRight,
  FileQuestion,
  Globe,
  Languages,
  LayoutDashboard,
  ListChecks,
  LogOut,
  MoreVertical,
  ScrollText,
  ShieldBan,
  Moon,
  Network,
  Rocket,
  Settings,
  ShieldCheck,
  Split,
  Sun,
  Users,
  Waypoints,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Toaster } from '@/components/ui/toaster'
import { api } from '@/lib/api'
import { useMe } from '@/lib/use-me'
import { useTheme } from '@/theme/theme-context'

// The sidebar is grouped into labelled sections. The first section has no
// label (the dashboard sits on its own above the groups).
const NAV_SECTIONS = [
  {
    labelKey: null,
    items: [{ to: '/', labelKey: 'nav.dashboard', icon: LayoutDashboard, exact: true }],
  },
  {
    labelKey: 'nav.sections.hosts',
    items: [
      { to: '/hosts', labelKey: 'nav.proxyHosts', icon: Globe, exact: false },
      { to: '/redirect-hosts', labelKey: 'nav.redirectHosts', icon: CornerUpRight, exact: false },
      { to: '/dead-hosts', labelKey: 'nav.deadHosts', icon: FileQuestion, exact: false },
      { to: '/streams', labelKey: 'nav.streams', icon: Network, exact: false },
      { to: '/sni-routers', labelKey: 'nav.sniRouters', icon: Split, exact: false },
    ],
  },
  {
    labelKey: 'nav.sections.certificates',
    items: [
      { to: '/certificates', labelKey: 'nav.certificates', icon: ShieldCheck, exact: false },
      { to: '/dns-providers', labelKey: 'nav.dnsProviders', icon: Cloud, exact: false },
    ],
  },
  {
    labelKey: 'nav.sections.security',
    items: [
      { to: '/access-lists', labelKey: 'nav.accessLists', icon: ListChecks, exact: false },
      { to: '/blocklist', labelKey: 'nav.blocklist', icon: ShieldBan, exact: false },
    ],
  },
  {
    labelKey: 'nav.sections.admin',
    items: [
      { to: '/apply', labelKey: 'nav.apply', icon: Rocket, exact: false },
      { to: '/users', labelKey: 'nav.users', icon: Users, exact: false, adminOnly: true },
      { to: '/audit', labelKey: 'nav.audit', icon: ScrollText, exact: false, adminOnly: true },
      { to: '/settings', labelKey: 'nav.settings', icon: Settings, exact: false },
    ],
  },
] as const

export function AppShell() {
  const { t } = useTranslation()
  const { data: me } = useMe()
  const isAdmin = me?.role === 'admin'

  return (
    <div className="flex min-h-svh">
      <aside className="hidden w-60 shrink-0 flex-col border-r bg-muted/20 md:flex">
        <div className="flex h-14 items-center gap-2 border-b px-4 font-semibold">
          <Waypoints className="size-5" aria-hidden="true" />
          {t('app.name')}
        </div>
        <nav className="flex flex-1 flex-col gap-4 overflow-y-auto p-3">
          {NAV_SECTIONS.map((section) => {
            const items = section.items.filter(
              (item) => isAdmin || !('adminOnly' in item),
            )
            if (items.length === 0) {
              return null
            }
            return (
              <div key={section.labelKey ?? 'main'} className="flex flex-col gap-1">
                {section.labelKey && (
                  <div className="px-3 pb-1 text-xs font-medium uppercase tracking-wider text-muted-foreground/70">
                    {t(section.labelKey)}
                  </div>
                )}
                {items.map((item) => (
                  <Link
                    key={item.to}
                    to={item.to}
                    activeOptions={{ exact: item.exact }}
                    className="flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    activeProps={{ className: 'bg-muted text-foreground' }}
                  >
                    <item.icon className="size-4" aria-hidden="true" />
                    {t(item.labelKey)}
                  </Link>
                ))}
              </div>
            )
          })}
        </nav>
        <SidebarFooter />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <MobileHeader />
        <main className="flex-1 p-4 lg:p-6">
          {me?.role === 'viewer' && (
            <div
              className="mb-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2 text-sm text-amber-700 dark:text-amber-300"
              role="status"
            >
              {t('common.readOnlyBanner')}
            </div>
          )}
          <Outlet />
        </main>
      </div>
      <Toaster />
    </div>
  )
}

function useLogout() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => api.logout(),
    onSettled: () => {
      queryClient.clear()
      void router.navigate({ to: '/login' })
    },
  })
}

/** Bottom of the sidebar (desktop): the signed-in user as a shadcn "nav-user"
 *  card that opens the session menu. */
function SidebarFooter() {
  return (
    <div className="mt-auto border-t p-2">
      <SidebarUser />
    </div>
  )
}

/** The sidebar is hidden on mobile, so a compact top bar carries the app name
 *  and the same user menu (avatar only). */
function MobileHeader() {
  const { t } = useTranslation()

  return (
    <header className="flex h-14 items-center justify-between gap-2 border-b px-4 md:hidden">
      <div className="flex items-center gap-2 font-semibold">
        <Waypoints className="size-5" aria-hidden="true" />
        {t('app.name')}
      </div>
      <SidebarUser compact />
    </header>
  )
}

function userInitials(email: string): string {
  const local = email.split('@')[0] || email
  return (local.slice(0, 2) || '?').toUpperCase()
}

/** shadcn "nav-user": an avatar + identity row (or just the avatar, on mobile)
 *  that opens a dropdown with theme, language, and logout. */
function SidebarUser({ compact = false }: { compact?: boolean }) {
  const { t, i18n } = useTranslation()
  const { data: me } = useMe()
  const { theme, toggleTheme } = useTheme()
  const logoutMutation = useLogout()

  const email = me?.email ?? ''
  const roleLabel = me ? t(`users.roles.${me.role}`) : ''
  const current = i18n.resolvedLanguage === 'ru' ? 'ru' : 'en'
  const next = current === 'ru' ? 'en' : 'ru'

  const avatar = (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-xs font-semibold text-primary">
      {userInitials(email)}
    </span>
  )
  const identity = (
    <div className="grid min-w-0 flex-1 text-left leading-tight">
      <span className="truncate text-sm font-medium">{email}</span>
      <span className="truncate text-xs text-muted-foreground">{roleLabel}</span>
    </div>
  )

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        {compact ? (
          <button
            type="button"
            aria-label={email}
            className="flex items-center justify-center rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {avatar}
          </button>
        ) : (
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-lg p-2 outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
          >
            {avatar}
            {identity}
            <MoreVertical
              className="size-4 shrink-0 text-muted-foreground"
              aria-hidden="true"
            />
          </button>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side={compact ? 'bottom' : 'right'}
        align="end"
        sideOffset={compact ? 8 : 12}
        className="min-w-56"
      >
        <div className="flex items-center gap-2 p-1.5">
          {avatar}
          {identity}
        </div>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault()
            toggleTheme()
          }}
        >
          {theme === 'dark' ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
          {theme === 'dark' ? t('header.themeLight') : t('header.themeDark')}
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault()
            void i18n.changeLanguage(next)
          }}
        >
          <Languages aria-hidden="true" />
          {next === 'ru' ? 'Русский' : 'English'}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onSelect={() => logoutMutation.mutate()}>
          <LogOut aria-hidden="true" />
          {t('header.logout')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

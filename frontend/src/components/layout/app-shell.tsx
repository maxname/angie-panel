import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, Outlet, useRouter } from '@tanstack/react-router'
import {
  CornerUpRight,
  FileQuestion,
  Globe,
  Languages,
  LayoutDashboard,
  ListChecks,
  LogOut,
  ShieldBan,
  Moon,
  Network,
  Rocket,
  Settings,
  ShieldCheck,
  Sun,
  Users,
  Waypoints,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { Toaster } from '@/components/ui/toaster'
import { api } from '@/lib/api'
import { useMe } from '@/lib/use-me'
import { useTheme } from '@/theme/theme-context'

const NAV_ITEMS = [
  { to: '/', labelKey: 'nav.dashboard', icon: LayoutDashboard, exact: true },
  { to: '/hosts', labelKey: 'nav.proxyHosts', icon: Globe, exact: false },
  { to: '/redirect-hosts', labelKey: 'nav.redirectHosts', icon: CornerUpRight, exact: false },
  { to: '/dead-hosts', labelKey: 'nav.deadHosts', icon: FileQuestion, exact: false },
  { to: '/streams', labelKey: 'nav.streams', icon: Network, exact: false },
  { to: '/certificates', labelKey: 'nav.certificates', icon: ShieldCheck, exact: false },
  { to: '/access-lists', labelKey: 'nav.accessLists', icon: ListChecks, exact: false },
  { to: '/blocklist', labelKey: 'nav.blocklist', icon: ShieldBan, exact: false },
  { to: '/apply', labelKey: 'nav.apply', icon: Rocket, exact: false },
  { to: '/users', labelKey: 'nav.users', icon: Users, exact: false, adminOnly: true },
  { to: '/settings', labelKey: 'nav.settings', icon: Settings, exact: false },
] as const

export function AppShell() {
  const { t } = useTranslation()
  const { data: me } = useMe()
  const isAdmin = me?.role === 'admin'
  const navItems = NAV_ITEMS.filter(
    (item) => isAdmin || !('adminOnly' in item),
  )

  return (
    <div className="flex min-h-svh">
      <aside className="hidden w-60 shrink-0 flex-col border-r bg-muted/20 md:flex">
        <div className="flex h-14 items-center gap-2 border-b px-4 font-semibold">
          <Waypoints className="size-5" aria-hidden="true" />
          {t('app.name')}
        </div>
        <nav className="flex flex-col gap-1 p-3">
          {navItems.map((item) => (
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
        </nav>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <AppHeader />
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

function AppHeader() {
  const { t } = useTranslation()
  const router = useRouter()
  const queryClient = useQueryClient()

  const meQuery = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: () => api.me(),
    staleTime: 5 * 60 * 1000,
    retry: false,
  })

  const logoutMutation = useMutation({
    mutationFn: () => api.logout(),
    onSettled: () => {
      queryClient.clear()
      void router.navigate({ to: '/login' })
    },
  })

  return (
    <header className="flex h-14 items-center justify-between gap-2 border-b px-4 lg:px-6">
      <div className="flex items-center gap-2 font-semibold md:hidden">
        <Waypoints className="size-5" aria-hidden="true" />
        {t('app.name')}
      </div>
      <div className="hidden font-semibold md:block">{t('app.name')}</div>
      <div className="flex items-center gap-1">
        {meQuery.data !== undefined && (
          <span className="mr-2 hidden items-center gap-2 text-sm text-muted-foreground sm:inline-flex">
            {meQuery.data.email}
            {meQuery.data.role === 'viewer' && (
              <Badge variant="outline" className="text-xs">
                {t('users.roles.viewer')}
              </Badge>
            )}
          </span>
        )}
        <LanguageToggle />
        <ThemeToggle />
        <Separator orientation="vertical" className="mx-1 h-6" />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => logoutMutation.mutate()}
          disabled={logoutMutation.isPending}
        >
          <LogOut aria-hidden="true" />
          {t('header.logout')}
        </Button>
      </div>
    </header>
  )
}

function ThemeToggle() {
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={toggleTheme}
      aria-label={t('header.toggleTheme')}
    >
      {theme === 'dark' ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
    </Button>
  )
}

function LanguageToggle() {
  const { t, i18n } = useTranslation()
  const current = i18n.resolvedLanguage === 'ru' ? 'ru' : 'en'
  const next = current === 'ru' ? 'en' : 'ru'

  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={() => void i18n.changeLanguage(next)}
      aria-label={t('header.switchLanguage')}
    >
      <Languages aria-hidden="true" />
      {current.toUpperCase()}
    </Button>
  )
}

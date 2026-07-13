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
  Menu,
  MoreVertical,
  PanelLeftClose,
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
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Toaster } from '@/components/ui/toaster'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { api } from '@/lib/api'
import { useMe } from '@/lib/use-me'
import { cn } from '@/lib/utils'
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

const COLLAPSE_KEY = 'sidebar-collapsed'

export function AppShell() {
  const { t } = useTranslation()
  const { data: me } = useMe()
  const isAdmin = me?.role === 'admin'

  // Desktop: collapse to an icon rail (persisted). Mobile: an off-canvas drawer.
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(COLLAPSE_KEY) === '1'
    } catch {
      return false
    }
  })
  const [mobileOpen, setMobileOpen] = useState(false)

  const toggleCollapsed = () =>
    setCollapsed((prev) => {
      const next = !prev
      try {
        localStorage.setItem(COLLAPSE_KEY, next ? '1' : '0')
      } catch {
        // ignore storage failures (private mode etc.)
      }
      return next
    })

  // Cmd/Ctrl+B toggles the desktop sidebar; Escape closes the mobile drawer.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'b' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        toggleCollapsed()
      } else if (event.key === 'Escape') {
        setMobileOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <div className="flex h-svh overflow-hidden">
      <DesktopSidebar
        collapsed={collapsed}
        isAdmin={isAdmin}
        onToggle={toggleCollapsed}
      />
      <MobileDrawer
        open={mobileOpen}
        isAdmin={isAdmin}
        onClose={() => setMobileOpen(false)}
      />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <MobileHeader onOpenMenu={() => setMobileOpen(true)} />
        <main className="flex-1 overflow-y-auto p-4 lg:p-6">
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

/** The desktop sidebar — full width, or a narrow icon rail with tooltips. */
function DesktopSidebar({
  collapsed,
  isAdmin,
  onToggle,
}: {
  collapsed: boolean
  isAdmin: boolean
  onToggle: () => void
}) {
  const { t } = useTranslation()

  return (
    <aside
      className={cn(
        'hidden shrink-0 flex-col border-r bg-muted/20 transition-[width] duration-200 md:flex',
        collapsed ? 'w-16' : 'w-60',
      )}
    >
      <div className="flex h-14 items-center gap-2 border-b px-3">
        {collapsed ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="mx-auto"
                aria-label={t('nav.expandSidebar')}
                onClick={onToggle}
              >
                <Waypoints className="size-5" aria-hidden="true" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="right">{t('nav.expandSidebar')}</TooltipContent>
          </Tooltip>
        ) : (
          <>
            <Waypoints className="size-5 shrink-0" aria-hidden="true" />
            <span className="flex-1 truncate font-semibold">{t('app.name')}</span>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('nav.collapseSidebar')}
              onClick={onToggle}
            >
              <PanelLeftClose className="size-4" aria-hidden="true" />
            </Button>
          </>
        )}
      </div>
      <NavSections collapsed={collapsed} isAdmin={isAdmin} />
      <div className="mt-auto border-t p-2">
        <SidebarUser compact={collapsed} />
      </div>
    </aside>
  )
}

/** Off-canvas sidebar for mobile, opened from the top bar. */
function MobileDrawer({
  open,
  isAdmin,
  onClose,
}: {
  open: boolean
  isAdmin: boolean
  onClose: () => void
}) {
  const { t } = useTranslation()
  if (!open) {
    return null
  }
  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <button
        type="button"
        aria-label={t('nav.closeMenu')}
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
      />
      <aside className="absolute inset-y-0 left-0 flex w-64 flex-col border-r bg-background shadow-xl">
        <div className="flex h-14 items-center gap-2 border-b px-4 font-semibold">
          <Waypoints className="size-5 shrink-0" aria-hidden="true" />
          {t('app.name')}
        </div>
        <NavSections collapsed={false} isAdmin={isAdmin} onNavigate={onClose} />
        <div className="mt-auto border-t p-2">
          <SidebarUser />
        </div>
      </aside>
    </div>
  )
}

/** The grouped nav links, shared by the desktop rail and the mobile drawer.
 *  When collapsed, labels and section headers are hidden and each icon gets a
 *  tooltip. */
function NavSections({
  collapsed,
  isAdmin,
  onNavigate,
}: {
  collapsed: boolean
  isAdmin: boolean
  onNavigate?: () => void
}) {
  const { t } = useTranslation()

  return (
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
            {section.labelKey && !collapsed && (
              <div className="px-3 pb-1 text-xs font-medium uppercase tracking-wider text-muted-foreground/70">
                {t(section.labelKey)}
              </div>
            )}
            {items.map((item) => {
              const label = t(item.labelKey)
              const link = (
                <Link
                  to={item.to}
                  activeOptions={{ exact: item.exact }}
                  onClick={onNavigate}
                  aria-label={collapsed ? label : undefined}
                  className={cn(
                    'flex items-center gap-2 rounded-lg py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground',
                    collapsed ? 'justify-center px-2' : 'px-3',
                  )}
                  activeProps={{ className: 'bg-muted text-foreground' }}
                >
                  <item.icon className="size-4 shrink-0" aria-hidden="true" />
                  {!collapsed && <span className="truncate">{label}</span>}
                </Link>
              )
              return collapsed ? (
                <Tooltip key={item.to}>
                  <TooltipTrigger asChild>{link}</TooltipTrigger>
                  <TooltipContent side="right">{label}</TooltipContent>
                </Tooltip>
              ) : (
                <div key={item.to}>{link}</div>
              )
            })}
          </div>
        )
      })}
    </nav>
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

/** A compact top bar for mobile (the sidebar is off-canvas there): a menu
 *  button, the app name, and the user menu. */
function MobileHeader({ onOpenMenu }: { onOpenMenu: () => void }) {
  const { t } = useTranslation()

  return (
    <header className="flex h-14 items-center justify-between gap-2 border-b px-4 md:hidden">
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('nav.openMenu')}
          onClick={onOpenMenu}
        >
          <Menu className="size-5" aria-hidden="true" />
        </Button>
        <div className="flex items-center gap-2 font-semibold">
          <Waypoints className="size-5" aria-hidden="true" />
          {t('app.name')}
        </div>
      </div>
      <SidebarUser compact side="bottom" />
    </header>
  )
}

function userInitials(email: string): string {
  const local = email.split('@')[0] || email
  return (local.slice(0, 2) || '?').toUpperCase()
}

/** shadcn "nav-user": an avatar + identity row (or just the avatar, when the
 *  sidebar is collapsed / on mobile) that opens a dropdown with theme,
 *  language, and logout. */
function SidebarUser({
  compact = false,
  side = 'right',
}: {
  compact?: boolean
  side?: 'right' | 'bottom'
}) {
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
            className="mx-auto flex items-center justify-center rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
        side={side}
        align="end"
        sideOffset={8}
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

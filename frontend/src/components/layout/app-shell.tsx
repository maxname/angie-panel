import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, Outlet, useRouter, useRouterState } from '@tanstack/react-router'
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
  Moon,
  Network,
  Rocket,
  ScrollText,
  Settings,
  ShieldBan,
  ShieldCheck,
  Split,
  Sun,
  Users,
  Waypoints,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { changeLanguage } from '@/i18n'

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
  useSidebar,
} from '@/components/ui/sidebar'
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

/** Number of staged-but-unapplied config changes (added + modified + removed
 *  managed files), polled from the apply preview. Drives the "unapplied
 *  changes" badge so a created host/cert isn't silently left un-applied.
 *  Shares the ['apply','preview'] cache with the Apply page. */
function usePendingChanges(): number {
  const { data } = useQuery({
    queryKey: ['apply', 'preview'],
    queryFn: () => api.getApplyPreview(),
    refetchInterval: 20_000,
    refetchOnWindowFocus: true,
    staleTime: 10_000,
  })
  if (!data) {
    return 0
  }
  return data.diff.added + data.diff.modified + data.diff.removed
}

export function AppShell() {
  const { t } = useTranslation()
  const { data: me } = useMe()
  const isAdmin = me?.role === 'admin'
  const pending = usePendingChanges()

  return (
    <SidebarProvider className="h-svh">
      <a
        href="#main-content"
        className="sr-only focus-visible:absolute focus-visible:top-2 focus-visible:left-2 focus-visible:z-50 focus-visible:rounded-md focus-visible:bg-background focus-visible:px-3 focus-visible:py-2 focus-visible:text-sm focus-visible:font-medium focus-visible:ring-2 focus-visible:ring-ring focus-visible:not-sr-only"
      >
        {t('nav.skipToContent')}
      </a>

      <AppSidebar isAdmin={isAdmin} pending={pending} />

      <SidebarInset className="overflow-hidden">
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger aria-label={t('nav.toggleSidebar')} />
        </header>
        {/* The scroll container stays full-width so the scrollbar sits at the
            viewport edge; the content inside is capped and centred so it never
            stretches uncomfortably wide on large screens. */}
        <div
          id="main-content"
          className="flex-1 overflow-y-auto overscroll-contain p-4 lg:p-6"
        >
          <div className="mx-auto w-full max-w-5xl">
            {me?.role === 'viewer' && (
              <div
                className="mb-4 rounded-lg border border-warning/30 bg-warning/10 px-4 py-2 text-sm text-warning"
                role="status"
              >
                {t('common.readOnlyBanner')}
              </div>
            )}
            <Outlet />
          </div>
        </div>
      </SidebarInset>

      <Toaster />
    </SidebarProvider>
  )
}

function AppSidebar({ isAdmin, pending }: { isAdmin: boolean; pending: number }) {
  const { t } = useTranslation()
  const pathname = useRouterState({ select: (s) => s.location.pathname })

  const isActive = (to: string, exact: boolean) =>
    exact ? pathname === to : pathname === to || pathname.startsWith(`${to}/`)

  return (
    <Sidebar collapsible="icon">
      {/* Fixed to the content header's height (h-14) with a matching border so
          the two dividers form one continuous line — otherwise the brand block
          grows/shrinks with the menu button and the rules never meet. */}
      <SidebarHeader className="h-14 justify-center border-b px-2 py-0">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to="/">
                {/* The icon lives in a size-8 square so that in icon mode it
                    exactly fills the collapsed 32px button (staying centred)
                    and the brand text is pushed past the clip. */}
                <div className="flex aspect-square size-8 shrink-0 items-center justify-center">
                  <Waypoints className="!size-5" aria-hidden="true" />
                </div>
                <span className="truncate text-base font-semibold" translate="no">
                  {t('app.name')}
                </span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        {NAV_SECTIONS.map((section) => {
          const items = section.items.filter(
            (item) => isAdmin || !('adminOnly' in item),
          )
          if (items.length === 0) {
            return null
          }
          return (
            <SidebarGroup key={section.labelKey ?? 'main'}>
              {section.labelKey && (
                <SidebarGroupLabel>{t(section.labelKey)}</SidebarGroupLabel>
              )}
              <SidebarGroupContent>
                <SidebarMenu>
                  {items.map((item) => {
                    const label = t(item.labelKey)
                    // The Apply item carries a badge when config changes are
                    // staged but not yet applied, so a created host/cert isn't
                    // silently left inactive.
                    const showBadge = item.to === '/apply' && pending > 0
                    const pendingLabel = t('nav.pendingChanges', { count: pending })
                    // Name the count for screen readers and for the collapsed
                    // tooltip — a bare "10" next to the item says nothing.
                    const describedLabel = showBadge
                      ? `${label} — ${pendingLabel}`
                      : label
                    return (
                      <SidebarMenuItem key={item.to}>
                        <SidebarMenuButton
                          asChild
                          isActive={isActive(item.to, item.exact)}
                          tooltip={describedLabel}
                        >
                          <Link
                            to={item.to}
                            aria-label={showBadge ? describedLabel : undefined}
                          >
                            <item.icon aria-hidden="true" />
                            <span>{label}</span>
                          </Link>
                        </SidebarMenuButton>
                        {showBadge && (
                          <>
                            {/* Visual only — the link's aria-label already names
                                the count, so don't announce it twice. */}
                            <SidebarMenuBadge
                              className="rounded-full bg-primary text-primary-foreground"
                              aria-hidden="true"
                            >
                              {pending}
                            </SidebarMenuBadge>
                            {/* SidebarMenuBadge is hidden in icon mode, so the
                                collapsed rail would lose the pending signal
                                entirely — keep a dot there instead. */}
                            <span
                              className="pointer-events-none absolute top-1 right-1 hidden size-2 rounded-full bg-primary ring-2 ring-sidebar group-data-[collapsible=icon]:block"
                              aria-hidden="true"
                            />
                          </>
                        )}
                      </SidebarMenuItem>
                    )
                  })}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )
        })}
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarUser />
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>

      <SidebarRail
        aria-label={t('nav.toggleSidebar')}
        title={t('nav.toggleSidebar')}
      />
    </Sidebar>
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

function userInitials(email: string): string {
  const local = email.split('@')[0] || email
  return (local.slice(0, 2) || '?').toUpperCase()
}

/** The footer identity row: an avatar + email/role that opens a dropdown with
 *  theme, language, and logout. Collapses to just the avatar in icon mode. */
function SidebarUser() {
  const { t, i18n } = useTranslation()
  const { data: me } = useMe()
  const { theme, toggleTheme } = useTheme()
  const { isMobile } = useSidebar()
  const logoutMutation = useLogout()

  const email = me?.email ?? ''
  const roleLabel = me ? t(`users.roles.${me.role}`) : ''
  const next = i18n.resolvedLanguage === 'ru' ? 'en' : 'ru'

  const avatar = (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-xs font-semibold text-primary">
      {userInitials(email)}
    </span>
  )

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <SidebarMenuButton
          size="lg"
          className="data-[state=open]:bg-sidebar-accent"
          aria-label={email}
        >
          {avatar}
          <div className="grid flex-1 text-left leading-tight">
            <span className="truncate text-sm font-medium">{email}</span>
            <span className="truncate text-xs text-muted-foreground">{roleLabel}</span>
          </div>
          <MoreVertical className="ml-auto size-4 text-muted-foreground" aria-hidden="true" />
        </SidebarMenuButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side={isMobile ? 'bottom' : 'right'}
        align="end"
        sideOffset={8}
        className="min-w-56"
      >
        <div className="flex items-center gap-2 p-1.5">
          {avatar}
          <div className="grid min-w-0 flex-1 text-left leading-tight">
            <span className="truncate text-sm font-medium">{email}</span>
            <span className="truncate text-xs text-muted-foreground">{roleLabel}</span>
          </div>
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
            void changeLanguage(next)
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

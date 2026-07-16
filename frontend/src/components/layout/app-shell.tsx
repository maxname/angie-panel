import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, Outlet, useRouter, useRouterState } from '@tanstack/react-router'
import {
  Cloud,
  CornerUpRight,
  FileQuestion,
  Globe,
  LayoutDashboard,
  ListChecks,
  LogOut,
  Network,
  Rocket,
  ScrollText,
  Settings,
  ShieldBan,
  ShieldCheck,
  Split,
  Users,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LanguageMenu, ThemeMenu } from '@/components/preference-menus'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import {
  Sidebar,
  SidebarContent,
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
} from '@/components/ui/sidebar'
import { Toaster } from '@/components/ui/toaster'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { api } from '@/lib/api'
import { useMe } from '@/lib/use-me'

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
 *  Shares the ['apply','preview'] cache with the Apply page.
 *
 *  null while we haven't heard back — the header says "applied" on 0, and
 *  saying it before we know would be a lie, however brief. */
function usePendingChanges(): number | null {
  const { data } = useQuery({
    queryKey: ['apply', 'preview'],
    queryFn: () => api.getApplyPreview(),
    refetchInterval: 20_000,
    refetchOnWindowFocus: true,
    staleTime: 10_000,
  })
  if (!data) {
    return null
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

      <AppSidebar isAdmin={isAdmin} pending={pending ?? 0} />

      <SidebarInset className="overflow-hidden">
        <header className="flex h-14 shrink-0 items-center gap-2 border-b bg-header-sheen px-4">
          <SidebarTrigger aria-label={t('nav.toggleSidebar')} />
          <HeaderStatus pending={pending} />
          <HeaderActions />
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
              // A warning alert in everything but name until now — hand-rolled
              // from the same tint, and so the one notice that stayed flat.
              // role=status, not alert: it's a standing condition, not news.
              <Alert variant="warning" role="status" className="mb-4">
                <AlertDescription>{t('common.readOnlyBanner')}</AlertDescription>
              </Alert>
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
      <SidebarHeader className="h-14 justify-center border-b bg-header-sheen px-2 py-0">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to="/">
                {/* The icon lives in a size-8 square so that in icon mode it
                    exactly fills the collapsed 32px button (staying centred)
                    and the brand text is pushed past the clip. */}
                <div className="flex aspect-square size-8 shrink-0 items-center justify-center">
                  {/* alt="" — decorative: the product name sits next to it as
                      text, and a screen reader shouldn't say it twice. */}
                  <img src="/logo.png" alt="" className="!size-7" />
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
                              // The badge carries its own indigo fill, so it must
                              // keep its own text colour: SidebarMenuBadge repaints
                              // the label sidebar-accent-foreground when the item is
                              // active or hovered, which is dark slate on indigo.
                              // Restating those variants lets tailwind-merge drop
                              // them rather than leaving it to class order.
                              className="rounded-full bg-primary text-primary-foreground peer-hover/menu-button:text-primary-foreground peer-data-active/menu-button:text-primary-foreground"
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

      <SidebarRail
        aria-label={t('nav.toggleSidebar')}
        title={t('nav.toggleSidebar')}
      />
    </Sidebar>
  )
}

/** Whether the running Angie config still matches the panel — the one thing
 *  worth stating on every page, since the panel is only a staging area until
 *  an apply. Pending changes link to where you fix that; otherwise it quietly
 *  confirms the two are in sync.
 *
 *  Reads the apply preview the badge already polls, so it costs no extra
 *  request. Angie's own health would be the other candidate here, but it is
 *  a process fork per poll on the box, and the dashboard already shows it. */
function HeaderStatus({ pending }: { pending: number | null }) {
  const { t } = useTranslation()

  if (pending === null) {
    return null
  }

  if (pending > 0) {
    return (
      <Link
        to="/apply"
        className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-sm font-medium text-foreground hover:bg-accent"
      >
        <span className="size-2 shrink-0 rounded-full bg-primary" aria-hidden="true" />
        <span className="truncate">
          {t('nav.pendingChanges', { count: pending })}
        </span>
      </Link>
    )
  }

  return (
    <div className="flex min-w-0 items-center gap-2 px-2 py-1 text-sm text-muted-foreground">
      <span className="size-2 shrink-0 rounded-full bg-success" aria-hidden="true" />
      <span className="truncate">{t('header.applied')}</span>
    </div>
  )
}

/** Session controls, parked on the right of the header. Language and theme are
 *  shared with the pre-auth screens; logout only makes sense here. */
function HeaderActions() {
  const { t } = useTranslation()
  const logoutMutation = useLogout()

  return (
    <div className="ml-auto flex items-center gap-1">
      <LanguageMenu />
      <ThemeMenu />

      <Separator orientation="vertical" className="mx-1 !h-4" />

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
            onClick={() => logoutMutation.mutate()}
            disabled={logoutMutation.isPending}
            aria-label={t('header.logout')}
          >
            <LogOut aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('header.logout')}</TooltipContent>
      </Tooltip>
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


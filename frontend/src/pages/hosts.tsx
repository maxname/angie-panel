import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  ArrowDown,
  ArrowUp,
  Loader2,
  MoreHorizontal,
  Plus,
  Server,
  ShieldCheck,
} from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  BulkActionsBar,
  SelectAllCheckbox,
  SelectRowCheckbox,
} from '@/components/bulk-actions'
import { DomainBadges } from '@/components/domain-badges'
import { HostEditorDialog } from '@/components/hosts/host-editor-dialog'
import { UptimeBar } from '@/components/hosts/uptime-bar'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { api, ApiError, type Cert, type Host, type UpstreamPool } from '@/lib/api'
import { lifecycleActions } from '@/lib/bulk'
import { toast } from '@/lib/toast'
import { useSelection, type Selection } from '@/lib/use-selection'

const HOST_BULK_ACTIONS = lifecycleActions<Host>({
  enable: api.enableHost,
  disable: api.disableHost,
  remove: api.deleteHost,
})

export function HostsPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<Host | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Host | null>(null)

  const hostsQuery = useQuery({
    queryKey: ['hosts'],
    queryFn: () => api.listHosts(),
  })
  // Needed to resolve each host's certificate_id to the cert it serves.
  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  // Sort by the first domain — the column operators actually scan. Order is
  // view state, not a query param: the list is already in memory and the
  // backend returns it in insertion order.
  const [sortAsc, setSortAsc] = useState(true)
  const sortedHosts = useMemo(() => {
    const rows = hostsQuery.data?.hosts ?? []
    // localeCompare, not <: domains are ASCII here, but numeric:'base' keeps
    // host10 after host9 instead of before it.
    return [...rows].sort((a, b) => {
      const cmp = (a.domains[0] ?? '').localeCompare(b.domains[0] ?? '', undefined, {
        numeric: true,
        sensitivity: 'base',
      })
      return sortAsc ? cmp : -cmp
    })
  }, [hostsQuery.data?.hosts, sortAsc])

  const selection = useSelection(sortedHosts)

  const certsById = new Map(
    (certsQuery.data?.certificates ?? []).map((cert) => [cert.id, cert]),
  )
  // Needed to name the shared pool a host proxies to (it has no host:port).
  const poolsQuery = useQuery({
    queryKey: ['upstream-pools'],
    queryFn: () => api.listUpstreamPools(),
  })
  const poolsById = new Map(
    (poolsQuery.data?.upstream_pools ?? []).map((pool) => [pool.id, pool]),
  )

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (host: Host) => {
    setEditing(host)
    setEditorOpen(true)
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">{t('hosts.title')}</h1>
        <Button onClick={openCreate}>
          <Plus aria-hidden="true" />
          {t('hosts.add')}
        </Button>
      </div>

      {hostsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : hostsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('hosts.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void hostsQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : hostsQuery.data.hosts.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('hosts.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-3">
          {/* Sort used to live in the column header; the card layout has none,
              so it moves to a control over the list. Still by first domain —
              the field operators scan. */}
          <div className="flex items-center justify-between pl-4">
            {/* A label, not a bare box: with no column headers above the
                cards, nothing else says what this one ticks. */}
            <label className="flex cursor-pointer items-center gap-3 text-sm text-muted-foreground">
              <SelectAllCheckbox selection={selection} />
              {t('bulk.selectAll')}
            </label>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSortAsc((v) => !v)}
              className="gap-1.5 text-muted-foreground"
              aria-label={t(sortAsc ? 'hosts.table.sortDesc' : 'hosts.table.sortAsc')}
            >
              {t('hosts.table.domains')}
              {sortAsc ? (
                <ArrowDown className="size-3.5" aria-hidden="true" />
              ) : (
                <ArrowUp className="size-3.5" aria-hidden="true" />
              )}
            </Button>
          </div>
          <Card className="p-0">
            <CardContent className="divide-y p-0">
              {sortedHosts.map((host) => (
                  <HostRow
                    key={host.id}
                    host={host}
                    cert={
                      host.certificate_id != null
                        ? certsById.get(host.certificate_id)
                        : undefined
                    }
                    pool={
                      host.upstream_pool_id != null
                        ? poolsById.get(host.upstream_pool_id)
                        : undefined
                    }
                    selection={selection}
                    onEdit={() => openEdit(host)}
                    onDelete={() => setDeleteTarget(host)}
                  />
              ))}
            </CardContent>
          </Card>
        </div>
      )}

      <BulkActionsBar
        selection={selection}
        label={(host) => host.domains[0] ?? `#${host.id}`}
        actions={HOST_BULK_ACTIONS}
        onChanged={() => void queryClient.invalidateQueries({ queryKey: ['hosts'] })}
        successNote={t('hosts.unappliedBody')}
      />

      <HostEditorDialog open={editorOpen} onOpenChange={setEditorOpen} host={editing} />
      <DeleteHostDialog
        host={deleteTarget}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

interface HostRowProps {
  host: Host
  cert: Cert | undefined
  /** The shared pool the host proxies to; undefined for a host with its own
   *  target, and while the pool list is still loading. */
  pool: UpstreamPool | undefined
  selection: Selection<Host>
  onEdit: () => void
  onDelete: () => void
}

function HostRow({ host, cert, pool, selection, onEdit, onDelete }: HostRowProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const toggleMutation = useMutation({
    mutationFn: async (): Promise<{ ok: true; enabled: boolean }> =>
      host.enabled ? api.disableHost(host.id) : api.enableHost(host.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['hosts'] })
      toast({
        title: t('hosts.unappliedTitle'),
        description: t('hosts.unappliedBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('hosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  // A host on a shared pool has no host:port of its own — name the pool.
  const onPool = host.upstream_pool_id !== null
  const target = onPool
    ? `${host.forward_scheme}://${pool?.name ?? '…'}`
    : `${host.forward_scheme}://${host.forward_host}:${host.forward_port}`
  // Only probed hosts get a bar. Config toggled off is not "down".
  const probed = host.enabled && host.health_checks.some((c) => c.enabled)

  return (
    <div className="space-y-2 p-4">
      {/* Line 1: the domains, and the actions menu pinned right. */}
      <div className="flex items-start gap-3">
        <div className="pt-1">
          <SelectRowCheckbox
            selection={selection}
            item={host}
            name={host.domains[0] ?? `#${host.id}`}
          />
        </div>
        <div className="min-w-0 flex-1">
          <DomainBadges domains={host.domains} secure={cert !== undefined} />
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              className="-mt-1 shrink-0"
              disabled={toggleMutation.isPending}
              aria-label={t('hosts.table.actions')}
            >
              {toggleMutation.isPending ? (
                <Loader2 className="animate-spin" aria-hidden="true" />
              ) : (
                <MoreHorizontal aria-hidden="true" />
              )}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              {t('hosts.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => toggleMutation.mutate()}>
              {host.enabled ? t('hosts.actions.disable') : t('hosts.actions.enable')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('hosts.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Line 2: status → target → certificate → uptime. Wraps on narrow
          screens rather than overflowing — what the columnar table could not
          do on a phone. */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 pl-7 text-sm">
        {host.enabled ? (
          <Badge variant="success">{t('hosts.status.enabled')}</Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('hosts.status.disabled')}
          </Badge>
        )}

        <span
          className="inline-flex items-center gap-1 font-mono text-xs text-muted-foreground"
          title={onPool ? t('hosts.table.pool') : undefined}
        >
          {onPool && (
            <Server
              className="size-3.5 shrink-0"
              role="img"
              aria-label={t('hosts.table.pool')}
            />
          )}
          {target}
        </span>

        {cert && (
          <span
            className="inline-flex items-center gap-1.5"
            title={cert.domains.join(', ')}
          >
            <ShieldCheck className="size-4 shrink-0 text-success" aria-hidden="true" />
            <span className="truncate">
              {cert.domains[0]}
              {cert.domains.length > 1 && (
                <span className="text-muted-foreground"> +{cert.domains.length - 1}</span>
              )}
            </span>
          </span>
        )}

        {probed && (
          <div className="ml-auto">
            <UptimeBar hostId={host.id} />
          </div>
        )}
      </div>
    </div>
  )
}

interface DeleteHostDialogProps {
  host: Host | null
  onOpenChange: (open: boolean) => void
}

function DeleteHostDialog({ host, onOpenChange }: DeleteHostDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteHost(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['hosts'] })
      toast({
        title: t('hosts.unappliedTitle'),
        description: t('hosts.unappliedBody'),
      })
      onOpenChange(false)
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('hosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={host !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('hosts.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('hosts.delete.body', { domain: host?.domains[0] ?? '' })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={deleteMutation.isPending}
          >
            {t('common.cancel')}
          </Button>
          <Button
            variant="destructive"
            onClick={() => host !== null && deleteMutation.mutate(host.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('hosts.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

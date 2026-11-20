import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Info, Loader2, MoreHorizontal, Plus } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  PoolServerList,
  UpstreamPoolEditorDialog,
} from '@/components/upstream-pools/upstream-pool-editor'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, ApiError, type UpstreamPool } from '@/lib/api'

export function UpstreamPoolsPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<UpstreamPool | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<UpstreamPool | null>(null)

  const poolsQuery = useQuery({
    queryKey: ['upstream-pools'],
    queryFn: () => api.listUpstreamPools(),
  })
  // Which hosts proxy to each pool — the number that says how far an edit
  // reaches, and why a pool cannot be deleted. Shares the ['hosts'] cache.
  const hostsQuery = useQuery({
    queryKey: ['hosts'],
    queryFn: () => api.listHosts(),
  })
  const domainsByPool = useMemo(() => {
    const map = new Map<number, string[]>()
    for (const host of hostsQuery.data?.hosts ?? []) {
      if (host.upstream_pool_id !== null) {
        const domains = map.get(host.upstream_pool_id) ?? []
        domains.push(host.domains[0] ?? `#${host.id}`)
        map.set(host.upstream_pool_id, domains)
      }
    }
    return map
  }, [hostsQuery.data?.hosts])

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (pool: UpstreamPool) => {
    setEditing(pool)
    setEditorOpen(true)
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('upstreamPools.title')}
        </h1>
        <Button onClick={openCreate}>
          <Plus aria-hidden="true" />
          {t('upstreamPools.add')}
        </Button>
      </div>

      <Alert variant="info">
        <Info aria-hidden="true" />
        <AlertDescription>{t('upstreamPools.info.body')}</AlertDescription>
      </Alert>

      {poolsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : poolsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('upstreamPools.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void poolsQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : poolsQuery.data.upstream_pools.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">
              {t('upstreamPools.empty')}
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('upstreamPools.table.name')}</TableHead>
                  <TableHead>{t('upstreamPools.table.servers')}</TableHead>
                  <TableHead>{t('upstreamPools.table.method')}</TableHead>
                  <TableHead>{t('upstreamPools.table.hosts')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">
                      {t('upstreamPools.table.actions')}
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {poolsQuery.data.upstream_pools.map((pool) => (
                  <UpstreamPoolRow
                    key={pool.id}
                    pool={pool}
                    usedBy={domainsByPool.get(pool.id) ?? []}
                    onEdit={() => openEdit(pool)}
                    onDelete={() => setDeleteTarget(pool)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <UpstreamPoolEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        pool={editing}
      />
      <DeleteUpstreamPoolDialog
        pool={deleteTarget}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

interface UpstreamPoolRowProps {
  pool: UpstreamPool
  /** First domain of every host that proxies to this pool. */
  usedBy: string[]
  onEdit: () => void
  onDelete: () => void
}

function UpstreamPoolRow({ pool, usedBy, onEdit, onDelete }: UpstreamPoolRowProps) {
  const { t } = useTranslation()

  return (
    <TableRow>
      <TableCell className="align-top font-medium">{pool.name}</TableCell>
      <TableCell className="align-top">
        <PoolServerList servers={pool.servers} />
      </TableCell>
      <TableCell className="align-top">
        <span className="font-mono text-xs">{pool.method}</span>
      </TableCell>
      <TableCell className="align-top">
        {usedBy.length === 0 ? (
          <span className="text-muted-foreground">
            {t('upstreamPools.unused')}
          </span>
        ) : (
          // The count is the headline; the names are a hover away.
          <span title={usedBy.join(', ')}>{usedBy.length}</span>
        )}
      </TableCell>
      <TableCell className="text-right align-top">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('upstreamPools.table.actions')}
            >
              <MoreHorizontal aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              {t('upstreamPools.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('upstreamPools.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteUpstreamPoolDialogProps {
  pool: UpstreamPool | null
  onOpenChange: (open: boolean) => void
}

export function DeleteUpstreamPoolDialog({
  pool,
  onOpenChange,
}: DeleteUpstreamPoolDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteUpstreamPool(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['upstream-pools'] })
      onOpenChange(false)
    },
  })

  // 409 upstream_pool_in_use carries a server message that lists the hosts
  // still proxying to this pool — show it verbatim.
  const serverError =
    deleteMutation.isError && deleteMutation.error instanceof ApiError
      ? deleteMutation.error.message
      : deleteMutation.isError
        ? t('common.error')
        : null

  return (
    <Dialog
      open={pool !== null}
      onOpenChange={(open) => {
        if (!open) {
          deleteMutation.reset()
        }
        onOpenChange(open)
      }}
    >
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('upstreamPools.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('upstreamPools.delete.body', { name: pool?.name ?? '' })}
          </DialogDescription>
        </DialogHeader>
        {serverError !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('upstreamPools.delete.failed')}</AlertTitle>
            <AlertDescription>{serverError}</AlertDescription>
          </Alert>
        )}
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
            onClick={() => pool !== null && deleteMutation.mutate(pool.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('upstreamPools.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

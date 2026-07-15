import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus, ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { HostEditorDialog } from '@/components/hosts/host-editor-dialog'
import { RouteCable } from '@/components/routing/route-cable'
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, ApiError, type Cert, type Host } from '@/lib/api'
import { toast } from '@/lib/toast'

export function HostsPage() {
  const { t } = useTranslation()
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
  const certsById = new Map(
    (certsQuery.data?.certificates ?? []).map((cert) => [cert.id, cert]),
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
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('hosts.table.route')}</TableHead>
                  <TableHead>{t('hosts.table.certificate')}</TableHead>
                  <TableHead>{t('hosts.table.status')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('hosts.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {hostsQuery.data.hosts.map((host) => (
                  <HostRow
                    key={host.id}
                    host={host}
                    cert={
                      host.certificate_id != null
                        ? certsById.get(host.certificate_id)
                        : undefined
                    }
                    onEdit={() => openEdit(host)}
                    onDelete={() => setDeleteTarget(host)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

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
  onEdit: () => void
  onDelete: () => void
}

function HostRow({ host, cert, onEdit, onDelete }: HostRowProps) {
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

  const target = `${host.forward_scheme}://${host.forward_host}:${host.forward_port}`

  return (
    <TableRow>
      <TableCell>
        <RouteCable
          state={host.enabled ? 'live' : 'off'}
          left={host.domains.map((domain) => (
            <Badge key={domain} variant="secondary" className="font-mono font-normal">
              {domain}
            </Badge>
          ))}
          right={target}
        />
      </TableCell>
      <TableCell>
        {cert ? (
          <span
            className="inline-flex items-center gap-1.5 text-sm"
            title={cert.domains.join(', ')}
          >
            <ShieldCheck
              className="size-4 shrink-0 text-emerald-600 dark:text-emerald-400"
              aria-hidden="true"
            />
            <span className="truncate">
              {cert.domains[0]}
              {cert.domains.length > 1 && (
                <span className="text-muted-foreground">
                  {' '}
                  +{cert.domains.length - 1}
                </span>
              )}
            </span>
          </span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell>
        {host.enabled ? (
          <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
            {t('hosts.status.enabled')}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('hosts.status.disabled')}
          </Badge>
        )}
      </TableCell>
      <TableCell className="text-right">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
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
      </TableCell>
    </TableRow>
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

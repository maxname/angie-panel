import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import {
  DomainChipsField,
  HostAdvancedField,
  HostSslFields,
  SslBadge,
  type SslToggles,
} from '@/components/hosts/host-editor-fields'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api, ApiError, type DeadHost, type DeadHostInput } from '@/lib/api'
import { toast } from '@/lib/toast'

export function DeadHostsPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<DeadHost | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<DeadHost | null>(null)

  const hostsQuery = useQuery({
    queryKey: ['dead-hosts'],
    queryFn: () => api.listDeadHosts(),
  })

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (host: DeadHost) => {
    setEditing(host)
    setEditorOpen(true)
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('deadHosts.title')}
        </h1>
        <Button onClick={openCreate}>
          <Plus aria-hidden="true" />
          {t('deadHosts.add')}
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
              {t('deadHosts.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void hostsQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : hostsQuery.data.dead_hosts.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('deadHosts.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('deadHosts.table.domains')}</TableHead>
                  <TableHead>{t('deadHosts.table.behaviour')}</TableHead>
                  <TableHead>{t('deadHosts.table.ssl')}</TableHead>
                  <TableHead>{t('deadHosts.table.status')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('deadHosts.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {hostsQuery.data.dead_hosts.map((host) => (
                  <DeadHostRow
                    key={host.id}
                    host={host}
                    onEdit={() => openEdit(host)}
                    onDelete={() => setDeleteTarget(host)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <DeadHostEditorDialog open={editorOpen} onOpenChange={setEditorOpen} host={editing} />
      <DeleteDeadHostDialog
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

interface DeadHostRowProps {
  host: DeadHost
  onEdit: () => void
  onDelete: () => void
}

function DeadHostRow({ host, onEdit, onDelete }: DeadHostRowProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const toggleMutation = useMutation({
    mutationFn: async (): Promise<{ ok: true; enabled: boolean }> =>
      host.enabled ? api.disableDeadHost(host.id) : api.enableDeadHost(host.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dead-hosts'] })
      toast({
        title: t('deadHosts.unappliedTitle'),
        description: t('deadHosts.unappliedBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('deadHosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <TableRow>
      <TableCell>
        <div className="flex flex-wrap gap-1">
          {host.domains.map((domain) => (
            <Badge key={domain} variant="secondary" className="font-mono font-normal">
              {domain}
            </Badge>
          ))}
        </div>
      </TableCell>
      <TableCell>
        <span className="text-muted-foreground">{t('deadHosts.table.returns404')}</span>
      </TableCell>
      <TableCell>
        <SslBadge hasCertificate={host.certificate_id !== null} />
      </TableCell>
      <TableCell>
        {host.enabled ? (
          <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
            {t('deadHosts.status.enabled')}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('deadHosts.status.disabled')}
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
              aria-label={t('deadHosts.table.actions')}
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
              {t('deadHosts.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => toggleMutation.mutate()}>
              {host.enabled
                ? t('deadHosts.actions.disable')
                : t('deadHosts.actions.enable')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('deadHosts.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteDeadHostDialogProps {
  host: DeadHost | null
  onOpenChange: (open: boolean) => void
}

export function DeleteDeadHostDialog({
  host,
  onOpenChange,
}: DeleteDeadHostDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteDeadHost(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dead-hosts'] })
      toast({
        title: t('deadHosts.unappliedTitle'),
        description: t('deadHosts.unappliedBody'),
      })
      onOpenChange(false)
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('deadHosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={host !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('deadHosts.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('deadHosts.delete.body', { domain: host?.domains[0] ?? '' })}
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
            {t('deadHosts.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface DeadHostEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The host being edited, or null when creating a new one. */
  host: DeadHost | null
}

function DeadHostEditorDialog({
  open,
  onOpenChange,
  host,
}: DeadHostEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {host === null
              ? t('deadHosts.editor.createTitle')
              : t('deadHosts.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>{t('deadHosts.editor.description')}</DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target host changes so state resets. */}
        <DeadHostEditorForm
          key={host?.id ?? 'new'}
          host={host}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface FormState {
  domains: string[]
  certificate_id: number | null
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  http2: boolean
  advanced_snippet: string
}

function initialState(host: DeadHost | null): FormState {
  if (host === null) {
    return {
      domains: [],
      certificate_id: null,
      force_ssl: false,
      hsts: false,
      hsts_subdomains: false,
      http2: true,
      advanced_snippet: '',
    }
  }
  return {
    domains: [...host.domains],
    certificate_id: host.certificate_id,
    force_ssl: host.force_ssl,
    hsts: host.hsts,
    hsts_subdomains: host.hsts_subdomains,
    http2: host.http2,
    advanced_snippet: host.advanced_snippet ?? '',
  }
}

interface DeadHostEditorFormProps {
  host: DeadHost | null
  onDone: () => void
}

export function DeadHostEditorForm({ host, onDone }: DeadHostEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const [form, setForm] = useState<FormState>(() => initialState(host))
  const [clientError, setClientError] = useState<string | null>(null)
  const [tab, setTab] = useState('details')

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const mutation = useMutation({
    mutationFn: (input: DeadHostInput) =>
      host === null
        ? api.createDeadHost(input)
        : api.updateDeadHost(host.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dead-hosts'] })
      toast({
        title: t('deadHosts.unappliedTitle'),
        description: t('deadHosts.unappliedBody'),
      })
      onDone()
    },
  })

  // domain_conflict and snippets_disabled carry a descriptive server message we
  // surface verbatim; invalid_domain maps to a readable inline message.
  const serverError = useMemo<{ domains?: string; form?: string }>(() => {
    if (!mutation.isError) {
      return {}
    }
    if (mutation.error instanceof ApiError) {
      const { code, message } = mutation.error
      if (code === 'invalid_domain') {
        return { domains: t('deadHosts.editor.errInvalidDomain') }
      }
      return { form: message }
    }
    return { form: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setClientError(null)

    if (form.domains.length === 0) {
      setClientError(t('deadHosts.editor.noDomains'))
      setTab('details')
      return
    }

    const input: DeadHostInput = {
      domains: form.domains,
      certificate_id: form.certificate_id,
      force_ssl: form.force_ssl,
      hsts: form.hsts,
      hsts_subdomains: form.hsts_subdomains,
      http2: form.http2,
      advanced_snippet:
        form.advanced_snippet.trim() === '' ? null : form.advanced_snippet,
      enabled: host === null ? true : host.enabled,
    }

    mutation.mutate(input)
  }

  const sslToggles: SslToggles = {
    force_ssl: form.force_ssl,
    http2: form.http2,
    hsts: form.hsts,
    hsts_subdomains: form.hsts_subdomains,
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="details">
            {t('deadHosts.editor.tabs.details')}
          </TabsTrigger>
          <TabsTrigger value="ssl">{t('deadHosts.editor.tabs.ssl')}</TabsTrigger>
          <TabsTrigger value="advanced">
            {t('deadHosts.editor.tabs.advanced')}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="details" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('deadHosts.editor.explainer')}
          </p>
          <div className="space-y-1">
            <DomainChipsField
              id="dead-domain-input"
              domains={form.domains}
              onChange={(domains) => patch({ domains })}
            />
            {serverError.domains !== undefined && (
              <p role="alert" className="text-sm text-destructive">
                {serverError.domains}
              </p>
            )}
          </div>
        </TabsContent>

        <TabsContent value="ssl">
          <HostSslFields
            idPrefix="dead"
            certificateId={form.certificate_id}
            onCertificateChange={(id) => patch({ certificate_id: id })}
            toggles={sslToggles}
            onToggle={patch}
          />
        </TabsContent>

        <TabsContent value="advanced">
          <HostAdvancedField
            id="dead-advanced-snippet"
            value={form.advanced_snippet}
            onChange={(value) => patch({ advanced_snippet: value })}
          />
        </TabsContent>
      </Tabs>

      {clientError !== null && (
        <p role="alert" className="text-sm text-destructive">
          {clientError}
        </p>
      )}
      {serverError.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('deadHosts.editor.saveFailed')}</AlertTitle>
          <AlertDescription>{serverError.form}</AlertDescription>
        </Alert>
      )}

      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onDone}
          disabled={mutation.isPending}
        >
          {t('common.cancel')}
        </Button>
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending && <Loader2 className="animate-spin" aria-hidden="true" />}
          {t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

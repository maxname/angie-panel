import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import {
  DomainChipsField,
  HostAdvancedField,
  HostSslFields,
  SslBadge,
  ToggleRow,
  type SslToggles,
} from '@/components/hosts/host-editor-fields'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { DomainBadges } from '@/components/domain-badges'
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
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  api,
  ApiError,
  type RedirectForwardScheme,
  type RedirectHost,
  type RedirectHostInput,
} from '@/lib/api'
import { useIsDirty } from '@/lib/use-dirty'
import { toast } from '@/lib/toast'

export function RedirectHostsPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<RedirectHost | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<RedirectHost | null>(null)

  const hostsQuery = useQuery({
    queryKey: ['redirect-hosts'],
    queryFn: () => api.listRedirectHosts(),
  })

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (host: RedirectHost) => {
    setEditing(host)
    setEditorOpen(true)
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('redirectHosts.title')}
        </h1>
        <Button onClick={openCreate}>
          <Plus aria-hidden="true" />
          {t('redirectHosts.add')}
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
              {t('redirectHosts.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void hostsQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : hostsQuery.data.redirect_hosts.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('redirectHosts.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('redirectHosts.table.domains')}</TableHead>
                  <TableHead>{t('redirectHosts.table.target')}</TableHead>
                  <TableHead>{t('redirectHosts.table.ssl')}</TableHead>
                  <TableHead>{t('redirectHosts.table.status')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('redirectHosts.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {hostsQuery.data.redirect_hosts.map((host) => (
                  <RedirectHostRow
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

      <RedirectHostEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        host={editing}
      />
      <DeleteRedirectHostDialog
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

/** "301 → https://new.example.com"; the scheme prefix is dropped for "auto". */
function formatTarget(host: RedirectHost): string {
  const prefix =
    host.forward_scheme === 'auto' ? '' : `${host.forward_scheme}://`
  return `${host.forward_http_code} → ${prefix}${host.forward_domain}`
}

interface RedirectHostRowProps {
  host: RedirectHost
  onEdit: () => void
  onDelete: () => void
}

function RedirectHostRow({ host, onEdit, onDelete }: RedirectHostRowProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const toggleMutation = useMutation({
    mutationFn: async (): Promise<{ ok: true; enabled: boolean }> =>
      host.enabled
        ? api.disableRedirectHost(host.id)
        : api.enableRedirectHost(host.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['redirect-hosts'] })
      toast({
        title: t('redirectHosts.unappliedTitle'),
        description: t('redirectHosts.unappliedBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('redirectHosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <TableRow>
      <TableCell>
        <DomainBadges
          domains={host.domains}
          secure={host.certificate_id !== null}
        />
      </TableCell>
      <TableCell>
        <span className="font-mono text-xs">{formatTarget(host)}</span>
      </TableCell>
      <TableCell>
        <SslBadge hasCertificate={host.certificate_id !== null} />
      </TableCell>
      <TableCell>
        {host.enabled ? (
          <Badge variant="success">
            {t('redirectHosts.status.enabled')}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('redirectHosts.status.disabled')}
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
              aria-label={t('redirectHosts.table.actions')}
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
              {t('redirectHosts.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => toggleMutation.mutate()}>
              {host.enabled
                ? t('redirectHosts.actions.disable')
                : t('redirectHosts.actions.enable')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('redirectHosts.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteRedirectHostDialogProps {
  host: RedirectHost | null
  onOpenChange: (open: boolean) => void
}

export function DeleteRedirectHostDialog({
  host,
  onOpenChange,
}: DeleteRedirectHostDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteRedirectHost(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['redirect-hosts'] })
      toast({
        title: t('redirectHosts.unappliedTitle'),
        description: t('redirectHosts.unappliedBody'),
      })
      onOpenChange(false)
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('redirectHosts.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={host !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('redirectHosts.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('redirectHosts.delete.body', { domain: host?.domains[0] ?? '' })}
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
            {t('redirectHosts.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface RedirectHostEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The host being edited, or null when creating a new one. */
  host: RedirectHost | null
}

function RedirectHostEditorDialog({
  open,
  onOpenChange,
  host,
}: RedirectHostEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {host === null
              ? t('redirectHosts.editor.createTitle')
              : t('redirectHosts.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>
            {t('redirectHosts.editor.description')}
          </DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target host changes so state resets. */}
        <RedirectHostEditorForm
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
  forward_scheme: RedirectForwardScheme
  forward_domain: string
  forward_http_code: string
  preserve_path: boolean
  block_exploits: boolean
  certificate_id: number | null
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  http2: boolean
  advanced_snippet: string
}

function initialState(host: RedirectHost | null): FormState {
  if (host === null) {
    return {
      domains: [],
      forward_scheme: 'auto',
      forward_domain: '',
      forward_http_code: '301',
      preserve_path: true,
      block_exploits: false,
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
    forward_scheme: host.forward_scheme,
    forward_domain: host.forward_domain,
    forward_http_code: String(host.forward_http_code),
    preserve_path: host.preserve_path,
    block_exploits: host.block_exploits,
    certificate_id: host.certificate_id,
    force_ssl: host.force_ssl,
    hsts: host.hsts,
    hsts_subdomains: host.hsts_subdomains,
    http2: host.http2,
    advanced_snippet: host.advanced_snippet ?? '',
  }
}

const HTTP_CODES = ['301', '302', '307', '308'] as const

interface FieldErrors {
  domains?: string
  forwardDomain?: string
  code?: string
  form?: string
}

interface RedirectHostEditorFormProps {
  host: RedirectHost | null
  onDone: () => void
}

export function RedirectHostEditorForm({
  host,
  onDone,
}: RedirectHostEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  // Keep the opening snapshot so the submit can tell whether there is
  // anything to save.
  const [initialForm] = useState<FormState>(() => initialState(host))
  const [form, setForm] = useState<FormState>(initialForm)
  // Only an edit can be "unchanged". A create form starts pristine by
  // definition, and its Save is what tells you which fields are missing.
  const canSave = useIsDirty(form, initialForm) || host === null
  const [clientError, setClientError] = useState<string | null>(null)
  const [tab, setTab] = useState('details')

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const mutation = useMutation({
    mutationFn: (input: RedirectHostInput) =>
      host === null
        ? api.createRedirectHost(input)
        : api.updateRedirectHost(host.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['redirect-hosts'] })
      toast({
        title: t('redirectHosts.unappliedTitle'),
        description: t('redirectHosts.unappliedBody'),
      })
      onDone()
    },
  })

  // Map server error codes to readable, field-scoped messages. domain_conflict
  // and snippets_disabled carry a descriptive server message we surface verbatim.
  const serverErrors = useMemo<FieldErrors>(() => {
    if (!mutation.isError) {
      return {}
    }
    if (mutation.error instanceof ApiError) {
      const { code, message } = mutation.error
      switch (code) {
        case 'invalid_domain':
          return { domains: t('redirectHosts.editor.errInvalidDomain') }
        case 'invalid_forward_domain':
          return { forwardDomain: t('redirectHosts.editor.errInvalidForwardDomain') }
        case 'invalid_redirect_code':
          return { code: t('redirectHosts.editor.errInvalidCode') }
        case 'domain_conflict':
        case 'snippets_disabled':
          return { form: message }
        default:
          return { form: message }
      }
    }
    return { form: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setClientError(null)

    if (form.domains.length === 0) {
      setClientError(t('redirectHosts.editor.noDomains'))
      setTab('details')
      return
    }
    if (form.forward_domain.trim() === '') {
      setClientError(t('redirectHosts.editor.noForwardDomain'))
      setTab('details')
      return
    }

    const input: RedirectHostInput = {
      domains: form.domains,
      forward_scheme: form.forward_scheme,
      forward_domain: form.forward_domain.trim(),
      forward_http_code: Number.parseInt(form.forward_http_code, 10),
      preserve_path: form.preserve_path,
      block_exploits: form.block_exploits,
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
            {t('redirectHosts.editor.tabs.details')}
          </TabsTrigger>
          <TabsTrigger value="ssl">{t('redirectHosts.editor.tabs.ssl')}</TabsTrigger>
          <TabsTrigger value="advanced">
            {t('redirectHosts.editor.tabs.advanced')}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="details" className="space-y-4">
          <div className="space-y-1">
            <DomainChipsField
              id="redirect-domain-input"
              domains={form.domains}
              onChange={(domains) => patch({ domains })}
            />
            {serverErrors.domains !== undefined && (
              <p role="alert" className="text-sm text-destructive">
                {serverErrors.domains}
              </p>
            )}
          </div>

          <div className="grid gap-4 sm:grid-cols-[8rem_1fr]">
            <div className="space-y-2">
              <Label htmlFor="redirect-scheme">
                {t('redirectHosts.editor.forwardScheme')}
              </Label>
              <Select
                value={form.forward_scheme}
                onValueChange={(value) =>
                  patch({ forward_scheme: value as RedirectForwardScheme })
                }
              >
                <SelectTrigger id="redirect-scheme">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">
                    {t('redirectHosts.editor.schemeAuto')}
                  </SelectItem>
                  <SelectItem value="http">http</SelectItem>
                  <SelectItem value="https">https</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="redirect-forward-domain">
                {t('redirectHosts.editor.forwardDomain')}
              </Label>
              <Input
                id="redirect-forward-domain"
                value={form.forward_domain}
                placeholder="new.example.com"
                onChange={(event) => patch({ forward_domain: event.target.value })}
              />
              {serverErrors.forwardDomain !== undefined && (
                <p role="alert" className="text-sm text-destructive">
                  {serverErrors.forwardDomain}
                </p>
              )}
            </div>
          </div>

          <div className="space-y-2 sm:max-w-[16rem]">
            <Label htmlFor="redirect-code">
              {t('redirectHosts.editor.httpCode')}
            </Label>
            <Select
              value={form.forward_http_code}
              onValueChange={(value) => patch({ forward_http_code: value })}
            >
              <SelectTrigger id="redirect-code">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {HTTP_CODES.map((code) => (
                  <SelectItem key={code} value={code}>
                    {t(`redirectHosts.editor.code.${code}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {serverErrors.code !== undefined && (
              <p role="alert" className="text-sm text-destructive">
                {serverErrors.code}
              </p>
            )}
          </div>

          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="redirect-preserve-path"
              label={t('redirectHosts.editor.preservePath')}
              checked={form.preserve_path}
              onChange={(checked) => patch({ preserve_path: checked })}
            />
            <ToggleRow
              id="redirect-block-exploits"
              label={t('redirectHosts.editor.blockExploits')}
              checked={form.block_exploits}
              onChange={(checked) => patch({ block_exploits: checked })}
            />
          </div>
        </TabsContent>

        <TabsContent value="ssl">
          <HostSslFields
            idPrefix="redirect"
            certificateId={form.certificate_id}
            onCertificateChange={(id) => patch({ certificate_id: id })}
            toggles={sslToggles}
            onToggle={patch}
          />
        </TabsContent>

        <TabsContent value="advanced">
          <HostAdvancedField
            id="redirect-advanced-snippet"
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
      {serverErrors.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('redirectHosts.editor.saveFailed')}</AlertTitle>
          <AlertDescription>{serverErrors.form}</AlertDescription>
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
        <Button type="submit" disabled={mutation.isPending || !canSave}>
          {mutation.isPending && <Loader2 className="animate-spin" aria-hidden="true" />}
          {t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

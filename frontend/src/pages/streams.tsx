import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus, Zap } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { ToggleRow } from '@/components/hosts/host-editor-fields'
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
import {
  api,
  ApiError,
  type Stream,
  type StreamInput,
  type StreamTls,
} from '@/lib/api'
import { toast } from '@/lib/toast'

export function StreamsPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<Stream | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Stream | null>(null)

  const streamsQuery = useQuery({
    queryKey: ['streams'],
    queryFn: () => api.listStreams(),
  })
  // The dashboard is the source of truth for whether the stream {} context is
  // active; reuse it so the enable banner shows only when it's actually off.
  const dashboardQuery = useQuery({
    queryKey: ['dashboard'],
    queryFn: () => api.getDashboard(),
  })

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (stream: Stream) => {
    setEditing(stream)
    setEditorOpen(true)
  }

  const contextActive = dashboardQuery.data?.streams.context_active ?? true
  const hasEnabledStreams =
    streamsQuery.data?.streams.some((s) => s.enabled) ?? false

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t('streams.title')}
          </h1>
          <Button className="shrink-0" onClick={openCreate}>
            <Plus aria-hidden="true" />
            {t('streams.add')}
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">{t('streams.subtitle')}</p>
      </div>

      {!contextActive && <EnableContextBanner highlight={hasEnabledStreams} />}

      {streamsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : streamsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('streams.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void streamsQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : streamsQuery.data.streams.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('streams.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('streams.table.incomingPort')}</TableHead>
                  <TableHead>{t('streams.table.forward')}</TableHead>
                  <TableHead>{t('streams.table.protocol')}</TableHead>
                  <TableHead>{t('streams.table.status')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('streams.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {streamsQuery.data.streams.map((stream) => (
                  <StreamRow
                    key={stream.id}
                    stream={stream}
                    onEdit={() => openEdit(stream)}
                    onDelete={() => setDeleteTarget(stream)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <StreamEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        stream={editing}
      />
      <DeleteStreamDialog
        stream={deleteTarget}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

/** The one-time "Enable streams" banner shown when the context is inactive. */
function EnableContextBanner({ highlight }: { highlight: boolean }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const enableMutation = useMutation({
    mutationFn: () => api.enableStreamContext(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dashboard'] })
      toast({
        title: t('streams.enableBanner.succeeded'),
        description: t('streams.enableBanner.succeededBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('streams.enableBanner.failed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Alert variant={highlight ? 'warning' : 'default'}>
      <Zap aria-hidden="true" />
      <AlertTitle>{t('streams.enableBanner.title')}</AlertTitle>
      <AlertDescription className="flex flex-col items-start gap-3">
        <span>{t('streams.enableBanner.body')}</span>
        <Button
          size="sm"
          onClick={() => enableMutation.mutate()}
          disabled={enableMutation.isPending}
        >
          {enableMutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {enableMutation.isPending
            ? t('streams.enableBanner.pending')
            : t('streams.enableBanner.action')}
        </Button>
      </AlertDescription>
    </Alert>
  )
}

function protocolLabel(stream: Stream, t: (key: string) => string): string {
  if (stream.tcp && stream.udp) {
    return t('streams.protocol.both')
  }
  return stream.tcp ? t('streams.protocol.tcp') : t('streams.protocol.udp')
}

interface StreamRowProps {
  stream: Stream
  onEdit: () => void
  onDelete: () => void
}

function StreamRow({ stream, onEdit, onDelete }: StreamRowProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const toggleMutation = useMutation({
    mutationFn: async (): Promise<{ ok: true; enabled: boolean }> =>
      stream.enabled ? api.disableStream(stream.id) : api.enableStream(stream.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['streams'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('streams.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <TableRow>
      <TableCell>
        <span className="font-mono text-sm">{stream.incoming_port}</span>
      </TableCell>
      <TableCell>
        <span className="font-mono text-xs">
          {stream.forward_host}:{stream.forward_port}
        </span>
      </TableCell>
      <TableCell>
        <div className="flex flex-wrap items-center gap-1">
          <Badge variant="secondary">{protocolLabel(stream, t)}</Badge>
          {stream.tls === 'terminate' && (
            <Badge className="bg-sky-600/15 text-sky-700 dark:bg-sky-400/15 dark:text-sky-400">
              {t('streams.tlsBadge')}
            </Badge>
          )}
        </div>
      </TableCell>
      <TableCell>
        {stream.enabled ? (
          <Badge variant="success">
            {t('streams.status.enabled')}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('streams.status.disabled')}
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
              aria-label={t('streams.table.actions')}
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
              {t('streams.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => toggleMutation.mutate()}>
              {stream.enabled
                ? t('streams.actions.disable')
                : t('streams.actions.enable')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('streams.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteStreamDialogProps {
  stream: Stream | null
  onOpenChange: (open: boolean) => void
}

export function DeleteStreamDialog({
  stream,
  onOpenChange,
}: DeleteStreamDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteStream(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['streams'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
      onOpenChange(false)
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('streams.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={stream !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('streams.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('streams.delete.body', { port: stream?.incoming_port ?? '' })}
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
            onClick={() => stream !== null && deleteMutation.mutate(stream.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('streams.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface StreamEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The stream being edited, or null when creating a new one. */
  stream: Stream | null
}

function StreamEditorDialog({
  open,
  onOpenChange,
  stream,
}: StreamEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {stream === null
              ? t('streams.editor.createTitle')
              : t('streams.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>{t('streams.editor.description')}</DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target stream changes so state resets. */}
        <StreamEditorForm
          key={stream?.id ?? 'new'}
          stream={stream}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface FormState {
  incoming_port: string
  forward_host: string
  forward_port: string
  tcp: boolean
  udp: boolean
  tls: StreamTls
  certificate_id: number | null
}

function initialState(stream: Stream | null): FormState {
  if (stream === null) {
    return {
      incoming_port: '',
      forward_host: '',
      forward_port: '',
      tcp: true,
      udp: false,
      tls: 'none',
      certificate_id: null,
    }
  }
  return {
    incoming_port: String(stream.incoming_port),
    forward_host: stream.forward_host,
    forward_port: String(stream.forward_port),
    tcp: stream.tcp,
    udp: stream.udp,
    tls: stream.tls,
    certificate_id: stream.certificate_id,
  }
}

interface FieldErrors {
  incomingPort?: string
  forwardHost?: string
  forwardPort?: string
  protocol?: string
  tls?: string
  form?: string
}

interface StreamEditorFormProps {
  stream: Stream | null
  onDone: () => void
}

export function StreamEditorForm({ stream, onDone }: StreamEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const [form, setForm] = useState<FormState>(() => initialState(stream))
  const [clientErrors, setClientErrors] = useState<FieldErrors>({})

  // Shares the ['certificates'] key so certs created on that page appear here.
  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  const certificates = certsQuery.data?.certificates ?? []

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  // TLS termination is a TCP-only SSL listener; selecting it forces TCP on and
  // UDP off so the form can't build a shape the backend will reject.
  const setTlsMode = (mode: StreamTls) =>
    setForm((prev) =>
      mode === 'terminate'
        ? { ...prev, tls: mode, tcp: true, udp: false }
        : { ...prev, tls: mode, certificate_id: null },
    )

  const mutation = useMutation({
    mutationFn: (input: StreamInput) =>
      stream === null
        ? api.createStream(input)
        : api.updateStream(stream.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['streams'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
      onDone()
    },
  })

  // Map server error codes to readable, field-scoped messages.
  const serverErrors = useMemo<FieldErrors>(() => {
    if (!mutation.isError) {
      return {}
    }
    if (mutation.error instanceof ApiError) {
      const { code, message } = mutation.error
      switch (code) {
        case 'invalid_port':
          return { form: t('streams.editor.errInvalidPort') }
        case 'invalid_forward_host':
          return { forwardHost: t('streams.editor.errInvalidForwardHost') }
        case 'no_protocol':
          return { protocol: t('streams.editor.noProtocol') }
        case 'cert_required':
          return { tls: t('streams.editor.tls.certRequired') }
        case 'tls_requires_tcp':
        case 'tls_tcp_only':
          return { tls: t('streams.editor.tls.tcpOnly') }
        case 'port_conflict':
          return { incomingPort: message }
        default:
          return { form: message }
      }
    }
    return { form: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const errors: FieldErrors = { ...serverErrors, ...clientErrors }

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const incoming = Number.parseInt(form.incoming_port, 10)
    const forward = Number.parseInt(form.forward_port, 10)
    const next: FieldErrors = {}
    if (!Number.isInteger(incoming) || incoming < 1 || incoming > 65535) {
      next.incomingPort = t('streams.editor.noIncomingPort')
    }
    if (form.forward_host.trim() === '') {
      next.forwardHost = t('streams.editor.noForwardHost')
    }
    if (!Number.isInteger(forward) || forward < 1 || forward > 65535) {
      next.forwardPort = t('streams.editor.noForwardPort')
    }
    if (!form.tcp && !form.udp) {
      next.protocol = t('streams.editor.noProtocol')
    }
    if (form.tls === 'terminate' && form.certificate_id === null) {
      next.tls = t('streams.editor.tls.certRequired')
    }
    setClientErrors(next)
    if (Object.keys(next).length > 0) {
      return
    }

    const input: StreamInput = {
      incoming_port: incoming,
      forward_host: form.forward_host.trim(),
      forward_port: forward,
      tcp: form.tcp,
      udp: form.udp,
      tls: form.tls,
      certificate_id: form.tls === 'terminate' ? form.certificate_id : null,
      enabled: stream === null ? true : stream.enabled,
    }
    mutation.mutate(input)
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      <div className="space-y-2">
        <Label htmlFor="stream-incoming-port">
          {t('streams.editor.incomingPort')}
        </Label>
        <Input
          id="stream-incoming-port"
          inputMode="numeric"
          value={form.incoming_port}
          placeholder="5432"
          onChange={(event) =>
            patch({ incoming_port: event.target.value.replace(/[^0-9]/g, '') })
          }
        />
        <p className="text-xs text-muted-foreground">
          {t('streams.editor.incomingPortHint')}
        </p>
        {errors.incomingPort !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.incomingPort}
          </p>
        )}
      </div>

      <div className="grid gap-4 sm:grid-cols-[1fr_8rem]">
        <div className="space-y-2">
          <Label htmlFor="stream-forward-host">
            {t('streams.editor.forwardHost')}
          </Label>
          <Input
            id="stream-forward-host"
            value={form.forward_host}
            placeholder="192.168.1.20"
            onChange={(event) => patch({ forward_host: event.target.value })}
          />
          {errors.forwardHost !== undefined && (
            <p role="alert" className="text-sm text-destructive">
              {errors.forwardHost}
            </p>
          )}
        </div>
        <div className="space-y-2">
          <Label htmlFor="stream-forward-port">
            {t('streams.editor.forwardPort')}
          </Label>
          <Input
            id="stream-forward-port"
            inputMode="numeric"
            value={form.forward_port}
            placeholder="5432"
            onChange={(event) =>
              patch({ forward_port: event.target.value.replace(/[^0-9]/g, '') })
            }
          />
          {errors.forwardPort !== undefined && (
            <p role="alert" className="text-sm text-destructive">
              {errors.forwardPort}
            </p>
          )}
        </div>
      </div>

      <div className="space-y-3 rounded-lg border p-3">
        <span className="text-sm font-medium">{t('streams.editor.protocol')}</span>
        <ToggleRow
          id="stream-tcp"
          label={t('streams.editor.tcp')}
          checked={form.tcp}
          onChange={(checked) => patch({ tcp: checked })}
        />
        <ToggleRow
          id="stream-udp"
          label={t('streams.editor.udp')}
          checked={form.udp}
          disabled={form.tls === 'terminate'}
          onChange={(checked) => patch({ udp: checked })}
        />
        {errors.protocol !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.protocol}
          </p>
        )}
      </div>

      <div className="space-y-3 rounded-lg border p-3">
        <div className="space-y-1">
          <span className="text-sm font-medium">
            {t('streams.editor.tls.title')}
          </span>
          <p className="text-xs text-muted-foreground">
            {t('streams.editor.tls.description')}
          </p>
        </div>
        <Select value={form.tls} onValueChange={(v) => setTlsMode(v as StreamTls)}>
          <SelectTrigger id="stream-tls-mode">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">{t('streams.editor.tls.modeNone')}</SelectItem>
            <SelectItem value="terminate">
              {t('streams.editor.tls.modeTerminate')}
            </SelectItem>
          </SelectContent>
        </Select>
        {form.tls === 'terminate' && (
          <div className="space-y-2">
            <Label htmlFor="stream-tls-cert">
              {t('streams.editor.tls.certificate')}
            </Label>
            <Select
              value={
                form.certificate_id === null ? '' : String(form.certificate_id)
              }
              onValueChange={(v) => patch({ certificate_id: Number.parseInt(v, 10) })}
            >
              <SelectTrigger id="stream-tls-cert">
                <SelectValue placeholder={t('streams.editor.tls.certPlaceholder')} />
              </SelectTrigger>
              <SelectContent>
                {certificates.map((cert) => (
                  <SelectItem key={cert.id} value={String(cert.id)}>
                    {cert.name} — {cert.domains.join(', ')}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {certsQuery.isError && (
              <p role="alert" className="text-sm text-destructive">
                {t('streams.editor.tls.certLoadFailed')}
              </p>
            )}
            <p className="text-xs text-muted-foreground">
              {t('streams.editor.tls.certHint')}
            </p>
          </div>
        )}
        {errors.tls !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.tls}
          </p>
        )}
      </div>

      {errors.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('streams.editor.saveFailed')}</AlertTitle>
          <AlertDescription>{errors.form}</AlertDescription>
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

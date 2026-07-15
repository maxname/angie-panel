import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus, X, Zap } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

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
  type SniRoute,
  type SniRouter,
  type SniRouterInput,
} from '@/lib/api'
import { toast } from '@/lib/toast'

export function SniRoutersPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<SniRouter | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<SniRouter | null>(null)

  const routersQuery = useQuery({
    queryKey: ['sni-routers'],
    queryFn: () => api.listSniRouters(),
  })
  // Routers live in the stream {} context, so reuse the dashboard's view of
  // whether it's active to show the one-time enable banner.
  const dashboardQuery = useQuery({
    queryKey: ['dashboard'],
    queryFn: () => api.getDashboard(),
  })

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }
  const openEdit = (router: SniRouter) => {
    setEditing(router)
    setEditorOpen(true)
  }

  const contextActive = dashboardQuery.data?.streams.context_active ?? true
  const hasEnabled = routersQuery.data?.sni_routers.some((r) => r.enabled) ?? false

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t('sniRouters.title')}
          </h1>
          <Button className="shrink-0" onClick={openCreate}>
            <Plus aria-hidden="true" />
            {t('sniRouters.add')}
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">
          {t('sniRouters.subtitle')}
        </p>
      </div>

      {!contextActive && hasEnabled && <EnableContextBanner />}

      {routersQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : routersQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('sniRouters.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void routersQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : routersQuery.data.sni_routers.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">
              {t('sniRouters.empty')}
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('sniRouters.table.name')}</TableHead>
                  <TableHead>{t('sniRouters.table.port')}</TableHead>
                  <TableHead>{t('sniRouters.table.routes')}</TableHead>
                  <TableHead>{t('sniRouters.table.status')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">
                      {t('sniRouters.table.actions')}
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {routersQuery.data.sni_routers.map((router) => (
                  <RouterRow
                    key={router.id}
                    router={router}
                    onEdit={() => openEdit(router)}
                    onDelete={() => setDeleteTarget(router)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <RouterEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        router={editing}
      />
      <DeleteRouterDialog
        router={deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      />
    </div>
  )
}

/** One-time "enable stream context" banner (shared behaviour with streams). */
function EnableContextBanner() {
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
    <Alert variant="warning">
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

interface RouterRowProps {
  router: SniRouter
  onEdit: () => void
  onDelete: () => void
}

function RouterRow({ router, onEdit, onDelete }: RouterRowProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const toggleMutation = useMutation({
    mutationFn: async (): Promise<{ ok: true; enabled: boolean }> =>
      router.enabled
        ? api.disableSniRouter(router.id)
        : api.enableSniRouter(router.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['sni-routers'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('sniRouters.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  const routeCount = router.routes.length + (router.default_host ? 1 : 0)

  return (
    <TableRow>
      <TableCell className="font-medium">{router.name}</TableCell>
      <TableCell>
        <span className="font-mono text-sm">{router.incoming_port}</span>
      </TableCell>
      <TableCell>
        <span className="text-sm text-muted-foreground">{routeCount}</span>
      </TableCell>
      <TableCell>
        {router.enabled ? (
          <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
            {t('sniRouters.status.enabled')}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('sniRouters.status.disabled')}
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
              aria-label={t('sniRouters.table.actions')}
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
              {t('sniRouters.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => toggleMutation.mutate()}>
              {router.enabled
                ? t('sniRouters.actions.disable')
                : t('sniRouters.actions.enable')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('sniRouters.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteRouterDialogProps {
  router: SniRouter | null
  onOpenChange: (open: boolean) => void
}

function DeleteRouterDialog({ router, onOpenChange }: DeleteRouterDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteSniRouter(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['sni-routers'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
      onOpenChange(false)
    },
    onError: (error) => {
      toast({
        variant: 'destructive',
        title: t('sniRouters.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={router !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('sniRouters.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('sniRouters.delete.body', { name: router?.name ?? '' })}
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
            onClick={() => router !== null && deleteMutation.mutate(router.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('sniRouters.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface RouterEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  router: SniRouter | null
}

function RouterEditorDialog({
  open,
  onOpenChange,
  router,
}: RouterEditorDialogProps) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-2xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {router === null
              ? t('sniRouters.editor.createTitle')
              : t('sniRouters.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>
            {t('sniRouters.editor.description')}
          </DialogDescription>
        </DialogHeader>
        {/* Remount so state resets whenever the target router changes. */}
        <RouterEditorForm
          key={router?.id ?? 'new'}
          router={router}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface RouteDraft {
  sni: string
  forward_host: string
  forward_port: string
}

interface FormState {
  name: string
  incoming_port: string
  routes: RouteDraft[]
  default_host: string
  default_port: string
}

function initialState(router: SniRouter | null): FormState {
  if (router === null) {
    return {
      name: '',
      incoming_port: '443',
      routes: [{ sni: '', forward_host: '', forward_port: '443' }],
      default_host: '',
      default_port: '',
    }
  }
  return {
    name: router.name,
    incoming_port: String(router.incoming_port),
    routes:
      router.routes.length > 0
        ? router.routes.map((r) => ({
            sni: r.sni,
            forward_host: r.forward_host,
            forward_port: String(r.forward_port),
          }))
        : [{ sni: '', forward_host: '', forward_port: '443' }],
    default_host: router.default_host,
    default_port: router.default_port > 0 ? String(router.default_port) : '',
  }
}

function RouterEditorForm({
  router,
  onDone,
}: {
  router: SniRouter | null
  onDone: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [form, setForm] = useState<FormState>(() => initialState(router))
  const [formError, setFormError] = useState<string | null>(null)

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const patchRoute = (index: number, partial: Partial<RouteDraft>) =>
    setForm((prev) => ({
      ...prev,
      routes: prev.routes.map((r, i) => (i === index ? { ...r, ...partial } : r)),
    }))

  const addRoute = () =>
    setForm((prev) => ({
      ...prev,
      routes: [...prev.routes, { sni: '', forward_host: '', forward_port: '443' }],
    }))

  const removeRoute = (index: number) =>
    setForm((prev) => ({
      ...prev,
      routes: prev.routes.filter((_, i) => i !== index),
    }))

  const mutation = useMutation({
    mutationFn: (body: SniRouterInput) =>
      router === null
        ? api.createSniRouter(body)
        : api.updateSniRouter(router.id, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['sni-routers'] })
      toast({
        title: t('streams.unappliedTitle'),
        description: t('streams.unappliedBody'),
      })
      onDone()
    },
    onError: (error) => {
      setFormError(error instanceof ApiError ? error.message : t('common.error'))
    },
  })

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    setFormError(null)

    // A row with a backend host but no SNI would be silently dropped by the
    // filter below — flag it instead of losing the operator's input. (The port
    // defaults to 443, so only a filled backend host signals real intent.)
    const partialRoute = form.routes.some(
      (r) => r.sni.trim() === '' && r.forward_host.trim() !== '',
    )
    if (partialRoute) {
      setFormError(t('sniRouters.editor.errPartialRoute'))
      return
    }

    const routes: SniRoute[] = form.routes
      .filter((r) => r.sni.trim() !== '')
      .map((r) => ({
        sni: r.sni.trim(),
        forward_host: r.forward_host.trim(),
        forward_port: Number.parseInt(r.forward_port, 10) || 0,
      }))

    const input: SniRouterInput = {
      name: form.name.trim(),
      incoming_port: Number.parseInt(form.incoming_port, 10) || 0,
      routes,
      default_host: form.default_host.trim(),
      default_port: Number.parseInt(form.default_port, 10) || 0,
      // Preserve the current enabled state on edit — the backend replaces the
      // row wholesale and defaults a missing `enabled` to true, which would
      // silently re-enable a disabled router (matches hosts/streams editors).
      enabled: router === null ? true : router.enabled,
    }
    mutation.mutate(input)
  }

  const port = Number.parseInt(form.incoming_port, 10)

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="sni-name">{t('sniRouters.editor.name')}</Label>
          <Input
            id="sni-name"
            value={form.name}
            placeholder={t('sniRouters.editor.namePlaceholder')}
            onChange={(e) => patch({ name: e.target.value })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="sni-port">{t('sniRouters.editor.port')}</Label>
          <Input
            id="sni-port"
            inputMode="numeric"
            value={form.incoming_port}
            onChange={(e) =>
              patch({ incoming_port: e.target.value.replace(/[^0-9]/g, '') })
            }
          />
        </div>
      </div>

      {port === 443 && (
        <Alert variant="warning">
          <AlertDescription>{t('sniRouters.editor.port443Warning')}</AlertDescription>
        </Alert>
      )}

      <div className="space-y-3">
        <div className="space-y-1">
          <span className="text-sm font-medium">
            {t('sniRouters.editor.routes')}
          </span>
          <p className="text-xs text-muted-foreground">
            {t('sniRouters.editor.routesHint')}
          </p>
        </div>
        {form.routes.map((route, index) => (
          <div key={index} className="flex items-start gap-2">
            <div className="grid flex-1 gap-2 sm:grid-cols-[1fr_1fr_auto]">
              <Input
                aria-label={t('sniRouters.editor.sni')}
                className="font-mono text-xs"
                spellCheck={false}
                placeholder="app.example.com"
                value={route.sni}
                onChange={(e) => patchRoute(index, { sni: e.target.value })}
              />
              <Input
                aria-label={t('sniRouters.editor.backendHost')}
                className="font-mono text-xs"
                spellCheck={false}
                placeholder="10.0.0.10"
                value={route.forward_host}
                onChange={(e) =>
                  patchRoute(index, { forward_host: e.target.value })
                }
              />
              <Input
                aria-label={t('sniRouters.editor.backendPort')}
                className="w-24 font-mono text-xs"
                inputMode="numeric"
                placeholder="443"
                value={route.forward_port}
                onChange={(e) =>
                  patchRoute(index, {
                    forward_port: e.target.value.replace(/[^0-9]/g, ''),
                  })
                }
              />
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t('sniRouters.editor.removeRoute')}
              disabled={form.routes.length === 1}
              onClick={() => removeRoute(index)}
            >
              <X aria-hidden="true" />
            </Button>
          </div>
        ))}
        <Button type="button" variant="outline" size="sm" onClick={addRoute}>
          <Plus aria-hidden="true" />
          {t('sniRouters.editor.addRoute')}
        </Button>
      </div>

      <div className="space-y-3">
        <div className="space-y-1">
          <span className="text-sm font-medium">
            {t('sniRouters.editor.defaultBackend')}
          </span>
          <p className="text-xs text-muted-foreground">
            {t('sniRouters.editor.defaultHint')}
          </p>
        </div>
        <div className="grid gap-2 sm:grid-cols-[1fr_auto]">
          <Input
            aria-label={t('sniRouters.editor.backendHost')}
            className="font-mono text-xs"
            spellCheck={false}
            placeholder={t('sniRouters.editor.defaultHostPlaceholder')}
            value={form.default_host}
            onChange={(e) => patch({ default_host: e.target.value })}
          />
          <Input
            aria-label={t('sniRouters.editor.backendPort')}
            className="w-24 font-mono text-xs"
            inputMode="numeric"
            placeholder="443"
            value={form.default_port}
            onChange={(e) =>
              patch({ default_port: e.target.value.replace(/[^0-9]/g, '') })
            }
          />
        </div>
      </div>

      {formError !== null && (
        <p role="alert" className="text-sm text-destructive">
          {formError}
        </p>
      )}

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          {t('common.cancel')}
        </Button>
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {router === null ? t('sniRouters.editor.create') : t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

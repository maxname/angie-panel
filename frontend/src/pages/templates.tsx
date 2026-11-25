import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  ChevronDown,
  Info,
  Loader2,
  MoreHorizontal,
  Plus,
  Server,
} from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  BulkActionsBar,
  SelectAllHead,
  SelectRowCell,
} from '@/components/bulk-actions'
import { HostEditorDialog } from '@/components/hosts/host-editor-dialog'
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
import {
  api,
  ApiError,
  type Cert,
  type HostTemplate,
  type StreamTemplate,
  type Template,
  type TemplateKind,
  type UpstreamPool,
} from '@/lib/api'
import { deleteAction } from '@/lib/bulk'
import { useSelection, type Selection } from '@/lib/use-selection'
import { StreamEditorDialog } from '@/pages/streams'

const TEMPLATE_BULK_ACTIONS = deleteAction<Template>(api.deleteTemplate)

/** Which template editor is up: one of a kind, on a template or on a blank. */
type Editing =
  | { kind: 'proxy_host'; template: HostTemplate | null }
  | { kind: 'stream'; template: StreamTemplate | null }

export function TemplatesPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  // What an editor works on is kept apart from whether it is open, so it does
  // not blank out while it fades away.
  const [editing, setEditing] = useState<Editing>({
    kind: 'proxy_host',
    template: null,
  })
  const [editorOpen, setEditorOpen] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<Template | null>(null)

  const templatesQuery = useQuery({
    queryKey: ['templates'],
    queryFn: () => api.listTemplates(),
  })
  const selection = useSelection(templatesQuery.data?.templates ?? [])
  // Only to put a name on what a template points at; the list reads fine
  // without them, so neither failing is worth an error state.
  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  const poolsQuery = useQuery({
    queryKey: ['upstream-pools'],
    queryFn: () => api.listUpstreamPools(),
  })
  const certsById = new Map(
    (certsQuery.data?.certificates ?? []).map((cert) => [cert.id, cert]),
  )
  const poolsById = new Map(
    (poolsQuery.data?.upstream_pools ?? []).map((pool) => [pool.id, pool]),
  )

  const open = (next: Editing) => {
    setEditing(next)
    setEditorOpen(true)
  }
  const openNew = (kind: TemplateKind) => open({ kind, template: null })
  const openEdit = (template: Template) =>
    open(
      template.kind === 'proxy_host'
        ? { kind: 'proxy_host', template }
        : { kind: 'stream', template },
    )

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('templates.title')}
        </h1>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button>
              <Plus aria-hidden="true" />
              {t('templates.add')}
              <ChevronDown aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => openNew('proxy_host')}>
              {t('templates.addHost')}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => openNew('stream')}>
              {t('templates.addStream')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <Alert variant="info">
        <Info aria-hidden="true" />
        <AlertDescription>{t('templates.info.body')}</AlertDescription>
      </Alert>

      {templatesQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : templatesQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('templates.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void templatesQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : templatesQuery.data.templates.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">
              {t('templates.empty')}
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <SelectAllHead selection={selection} />
                  <TableHead>{t('templates.table.name')}</TableHead>
                  <TableHead>{t('templates.table.kind')}</TableHead>
                  <TableHead>{t('templates.table.target')}</TableHead>
                  <TableHead>{t('templates.table.certificate')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">
                      {t('templates.table.actions')}
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {templatesQuery.data.templates.map((template) => (
                  <TemplateRow
                    key={template.id}
                    template={template}
                    selection={selection}
                    certsById={certsById}
                    poolsById={poolsById}
                    onEdit={() => openEdit(template)}
                    onDelete={() => setDeleteTarget(template)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <BulkActionsBar
        selection={selection}
        label={(template) => template.name}
        actions={TEMPLATE_BULK_ACTIONS}
        onChanged={() =>
          void queryClient.invalidateQueries({ queryKey: ['templates'] })
        }
      />

      {/* Both editors stay mounted; only the one of the kind being edited is
          ever open. */}
      <HostEditorDialog
        open={editorOpen && editing.kind === 'proxy_host'}
        onOpenChange={setEditorOpen}
        host={null}
        templateEdit={{
          template: editing.kind === 'proxy_host' ? editing.template : null,
        }}
      />
      <StreamEditorDialog
        open={editorOpen && editing.kind === 'stream'}
        onOpenChange={setEditorOpen}
        stream={null}
        templateEdit={{
          template: editing.kind === 'stream' ? editing.template : null,
        }}
      />
      <DeleteTemplateDialog
        template={deleteTarget}
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

interface TemplateRowProps {
  template: Template
  selection: Selection<Template>
  certsById: Map<number, Cert>
  poolsById: Map<number, UpstreamPool>
  onEdit: () => void
  onDelete: () => void
}

function TemplateRow({
  template,
  selection,
  certsById,
  poolsById,
  onEdit,
  onDelete,
}: TemplateRowProps) {
  const { t } = useTranslation()
  const { config } = template
  const pool =
    template.kind === 'proxy_host' && template.config.upstream_pool_id !== null
      ? poolsById.get(template.config.upstream_pool_id)
      : undefined
  const cert =
    config.certificate_id !== null
      ? certsById.get(config.certificate_id)
      : undefined

  return (
    <TableRow
      data-state={selection.isSelected(template.id) ? 'selected' : undefined}
    >
      <SelectRowCell selection={selection} item={template} name={template.name} />
      <TableCell className="font-medium">{template.name}</TableCell>
      <TableCell>
        <Badge variant="outline">{t(`templates.kind.${template.kind}`)}</Badge>
      </TableCell>
      <TableCell>
        {pool !== undefined ? (
          <span className="inline-flex items-center gap-1 font-mono text-xs">
            <Server
              className="size-3.5 shrink-0 text-muted-foreground"
              role="img"
              aria-label={t('hosts.table.pool')}
            />
            {pool.name}
          </span>
        ) : config.forward_host !== '' ? (
          <span className="font-mono text-xs">
            {template.kind === 'proxy_host'
              ? `${template.config.forward_scheme}://`
              : ''}
            {config.forward_host}:{config.forward_port}
          </span>
        ) : (
          // The target is the one setting a template may leave open.
          <span className="text-muted-foreground">
            {t('templates.targetOpen')}
          </span>
        )}
      </TableCell>
      <TableCell>
        {template.kind === 'proxy_host' &&
        template.config.new_certificate !== null ? (
          t('templates.certificateNew')
        ) : cert !== undefined ? (
          <span className="font-mono text-xs">{cert.domains[0] ?? cert.name}</span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell className="text-right">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('templates.table.actions')}
            >
              <MoreHorizontal aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              {t('templates.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('templates.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteTemplateDialogProps {
  template: Template | null
  onOpenChange: (open: boolean) => void
}

function DeleteTemplateDialog({ template, onOpenChange }: DeleteTemplateDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteTemplate(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['templates'] })
      onOpenChange(false)
    },
  })

  const serverError =
    deleteMutation.isError && deleteMutation.error instanceof ApiError
      ? deleteMutation.error.message
      : deleteMutation.isError
        ? t('common.error')
        : null

  return (
    <Dialog
      open={template !== null}
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          deleteMutation.reset()
        }
        onOpenChange(isOpen)
      }}
    >
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('templates.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('templates.delete.body', { name: template?.name ?? '' })}
          </DialogDescription>
        </DialogHeader>
        {serverError !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('templates.delete.failed')}</AlertTitle>
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
            onClick={() =>
              template !== null && deleteMutation.mutate(template.id)
            }
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('templates.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

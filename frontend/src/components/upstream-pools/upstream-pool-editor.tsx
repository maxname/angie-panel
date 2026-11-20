import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2, Plus, X } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
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
  api,
  ApiError,
  type BalanceMethod,
  type UpstreamPool,
  type UpstreamPoolInput,
  type UpstreamServer,
} from '@/lib/api'
import { toast } from '@/lib/toast'
import { useIsDirty } from '@/lib/use-dirty'

/** The servers of a pool, read-only: `host:port` plus whatever sets a member
 *  apart (weight, backup, down). Shared by the pool list and the host editor,
 *  so a pool reads the same wherever it is shown. */
export function PoolServerList({ servers }: { servers: UpstreamServer[] }) {
  const { t } = useTranslation()

  return (
    <ul className="space-y-1">
      {servers.map((server, index) => (
        <li key={index} className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span
            className={
              server.down
                ? 'font-mono text-xs text-muted-foreground line-through'
                : 'font-mono text-xs'
            }
          >
            {server.host}:{server.port}
          </span>
          {server.weight !== 1 && (
            <span className="text-xs text-muted-foreground">
              {t('upstreamPools.serverWeight', { weight: server.weight })}
            </span>
          )}
          {server.backup && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
              {t('hosts.editor.upstreams.backup')}
            </span>
          )}
          {server.down && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
              {t('hosts.editor.upstreams.down')}
            </span>
          )}
        </li>
      ))}
    </ul>
  )
}

interface UpstreamPoolEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The pool being edited, or null when creating a new one. */
  pool: UpstreamPool | null
  /** Called with the saved pool — lets the host editor select a pool it has
   *  just created without making the user find it in the list. */
  onSaved?: (pool: UpstreamPool) => void
}

export function UpstreamPoolEditorDialog({
  open,
  onOpenChange,
  pool,
  onSaved,
}: UpstreamPoolEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {pool === null
              ? t('upstreamPools.editor.createTitle')
              : t('upstreamPools.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>
            {t('upstreamPools.editor.description')}
          </DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target pool changes so state resets. */}
        <UpstreamPoolEditorForm
          key={pool?.id ?? 'new'}
          pool={pool}
          onSaved={onSaved}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface ServerDraft {
  host: string
  port: string
  weight: string
  backup: boolean
  down: boolean
}

interface FormState {
  name: string
  method: BalanceMethod
  max_fails: string
  fail_timeout_secs: string
  servers: ServerDraft[]
}

const EMPTY_SERVER: ServerDraft = {
  host: '',
  port: '',
  weight: '1',
  backup: false,
  down: false,
}

function initialState(pool: UpstreamPool | null): FormState {
  if (pool === null) {
    return {
      name: '',
      method: 'round_robin',
      max_fails: '1',
      fail_timeout_secs: '10',
      // A pool is nothing without a server, so the first row is already there.
      servers: [{ ...EMPTY_SERVER }],
    }
  }
  return {
    name: pool.name,
    method: pool.method,
    max_fails: String(pool.max_fails),
    fail_timeout_secs: String(pool.fail_timeout_secs),
    servers: pool.servers.map((server) => ({
      host: server.host,
      port: String(server.port),
      weight: String(server.weight),
      backup: server.backup,
      down: server.down,
    })),
  }
}

interface FieldErrors {
  name?: string
  form?: string
}

interface UpstreamPoolEditorFormProps {
  pool: UpstreamPool | null
  onDone: () => void
  onSaved?: (pool: UpstreamPool) => void
}

export function UpstreamPoolEditorForm({
  pool,
  onDone,
  onSaved,
}: UpstreamPoolEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const [initialForm] = useState<FormState>(() => initialState(pool))
  const [form, setForm] = useState<FormState>(initialForm)
  // Only an edit can be "unchanged". A create form starts pristine by
  // definition, and its Save is what tells you which fields are missing.
  const canSave = useIsDirty(form, initialForm) || pool === null
  const [clientErrors, setClientErrors] = useState<FieldErrors>({})

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const updateServer = (index: number, partial: Partial<ServerDraft>) =>
    patch({
      servers: form.servers.map((server, i) =>
        i === index ? { ...server, ...partial } : server,
      ),
    })

  const mutation = useMutation({
    mutationFn: (input: UpstreamPoolInput) =>
      pool === null
        ? api.createUpstreamPool(input)
        : api.updateUpstreamPool(pool.id, input),
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ['upstream-pools'] })
      toast({
        title: t('upstreamPools.editor.savedToastTitle'),
        description: t('upstreamPools.editor.savedToastBody'),
      })
      onSaved?.(saved)
      onDone()
    },
  })

  // Map the server's typed error codes onto the name field or a form-level alert.
  const serverErrors = useMemo<FieldErrors>(() => {
    if (!mutation.isError) {
      return {}
    }
    if (mutation.error instanceof ApiError) {
      const { code, message } = mutation.error
      if (code === 'invalid_name' || code === 'upstream_pool_name_taken') {
        return { name: message }
      }
      return { form: message }
    }
    return { form: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const errors: FieldErrors = {
    name: clientErrors.name ?? serverErrors.name,
    form: clientErrors.form ?? serverErrors.form,
  }

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    // This dialog is also opened from the host editor. React bubbles events
    // through portals along the component tree, not the DOM, so a submit here
    // must never reach a form this dialog happens to be rendered within —
    // saving a pool would save the host being edited along with it.
    event.stopPropagation()

    if (form.name.trim() === '') {
      setClientErrors({ name: t('upstreamPools.editor.noName') })
      return
    }
    if (form.servers.length === 0) {
      setClientErrors({ form: t('upstreamPools.editor.errNoServers') })
      return
    }
    for (const server of form.servers) {
      const port = Number.parseInt(server.port, 10)
      if (
        server.host.trim() === '' ||
        !Number.isInteger(port) ||
        port < 1 ||
        port > 65535
      ) {
        setClientErrors({ form: t('hosts.editor.upstreams.errServer') })
        return
      }
    }
    // Angie refuses both of these; say so here rather than at apply time.
    if (form.servers.every((server) => server.backup)) {
      setClientErrors({ form: t('upstreamPools.editor.errOnlyBackups') })
      return
    }
    if (
      form.method === 'ip_hash' &&
      form.servers.some((server) => server.backup)
    ) {
      setClientErrors({ form: t('hosts.editor.upstreams.errIpHashBackup') })
      return
    }
    setClientErrors({})

    mutation.mutate({
      name: form.name.trim(),
      method: form.method,
      max_fails: Number.parseInt(form.max_fails, 10) || 0,
      fail_timeout_secs: Number.parseInt(form.fail_timeout_secs, 10) || 10,
      servers: form.servers.map((server) => ({
        host: server.host.trim(),
        port: Number.parseInt(server.port, 10) || 0,
        weight: Number.parseInt(server.weight, 10) || 1,
        backup: server.backup,
        down: server.down,
      })),
    })
  }

  const digits = (value: string) => value.replace(/[^0-9]/g, '')

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      <div className="grid gap-4 sm:grid-cols-[1fr_14rem]">
        <div className="space-y-2">
          <Label htmlFor="pool-name">{t('upstreamPools.editor.name')}</Label>
          <Input
            id="pool-name"
            value={form.name}
            spellCheck={false}
            autoComplete="off"
            placeholder={t('upstreamPools.editor.namePlaceholder')}
            onChange={(event) => {
              patch({ name: event.target.value })
              setClientErrors((prev) => ({ ...prev, name: undefined }))
            }}
          />
          {errors.name !== undefined && (
            <p role="alert" className="text-sm text-destructive">
              {errors.name}
            </p>
          )}
        </div>
        <div className="space-y-2">
          <Label htmlFor="pool-method">
            {t('hosts.editor.upstreams.method')}
          </Label>
          <Select
            value={form.method}
            onValueChange={(value) => patch({ method: value as BalanceMethod })}
          >
            <SelectTrigger id="pool-method">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="round_robin">
                {t('hosts.editor.upstreams.roundRobin')}
              </SelectItem>
              <SelectItem value="least_conn">least_conn</SelectItem>
              <SelectItem value="ip_hash">ip_hash</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      <fieldset className="space-y-3 rounded-lg border p-3">
        <legend className="px-1 text-sm font-medium">
          {t('hosts.editor.upstreams.servers')}
        </legend>
        {form.servers.map((server, index) => (
          <div key={index} className="flex flex-wrap items-center gap-2">
            <Input
              id={`pool-server-host-${index}`}
              aria-label={t('hosts.editor.upstreams.serverHost')}
              placeholder="10.0.0.2"
              spellCheck={false}
              autoComplete="off"
              className="h-8 w-44"
              value={server.host}
              onChange={(event) =>
                updateServer(index, { host: event.target.value })
              }
            />
            <Input
              aria-label={t('hosts.editor.upstreams.serverPort')}
              inputMode="numeric"
              placeholder="8080"
              className="h-8 w-20"
              value={server.port}
              onChange={(event) =>
                updateServer(index, { port: digits(event.target.value) })
              }
            />
            <Input
              aria-label={t('hosts.editor.upstreams.weight')}
              inputMode="numeric"
              placeholder="1"
              className="h-8 w-14"
              value={server.weight}
              onChange={(event) =>
                updateServer(index, { weight: digits(event.target.value) })
              }
            />
            <label className="flex items-center gap-1 text-xs">
              <input
                type="checkbox"
                checked={server.backup}
                onChange={(event) =>
                  updateServer(index, { backup: event.target.checked })
                }
              />
              {t('hosts.editor.upstreams.backup')}
            </label>
            <label className="flex items-center gap-1 text-xs">
              <input
                type="checkbox"
                checked={server.down}
                onChange={(event) =>
                  updateServer(index, { down: event.target.checked })
                }
              />
              {t('hosts.editor.upstreams.down')}
            </label>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="ml-auto"
              aria-label={t('hosts.editor.upstreams.removeServer')}
              onClick={() =>
                patch({ servers: form.servers.filter((_, i) => i !== index) })
              }
            >
              <X aria-hidden="true" />
            </Button>
          </div>
        ))}
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() =>
            patch({ servers: [...form.servers, { ...EMPTY_SERVER }] })
          }
        >
          <Plus aria-hidden="true" />
          {t('hosts.editor.upstreams.addServer')}
        </Button>
      </fieldset>

      <fieldset className="space-y-3 rounded-lg border p-3">
        <legend className="px-1 text-sm font-medium">
          {t('hosts.editor.upstreams.health')}
        </legend>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="pool-max-fails">
              {t('hosts.editor.upstreams.maxFails')}
            </Label>
            <Input
              id="pool-max-fails"
              inputMode="numeric"
              value={form.max_fails}
              onChange={(event) =>
                patch({ max_fails: digits(event.target.value) })
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="pool-fail-timeout">
              {t('hosts.editor.upstreams.failTimeout')}
            </Label>
            <Input
              id="pool-fail-timeout"
              inputMode="numeric"
              value={form.fail_timeout_secs}
              onChange={(event) =>
                patch({ fail_timeout_secs: digits(event.target.value) })
              }
            />
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {t('hosts.editor.upstreams.healthHint')}
        </p>
      </fieldset>

      {errors.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('upstreamPools.editor.saveFailed')}</AlertTitle>
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
        <Button type="submit" disabled={mutation.isPending || !canSave}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

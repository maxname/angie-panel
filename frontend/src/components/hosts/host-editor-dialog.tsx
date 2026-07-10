import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
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
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import {
  api,
  ApiError,
  type ForwardScheme,
  type Host,
  type HostInput,
} from '@/lib/api'
import { isValidDomain } from '@/lib/domain'
import { toast } from '@/lib/toast'

interface LocationDraft {
  path: string
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: string
  rewrite: string
}

interface FormState {
  domains: string[]
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: string
  websockets_upgrade: boolean
  block_exploits: boolean
  cache_assets: boolean
  http2: boolean
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  trust_forwarded_proto: boolean
  certificate_id: number | null
  locations: LocationDraft[]
  advanced_snippet: string
}

function initialState(host: Host | null): FormState {
  if (host === null) {
    return {
      domains: [],
      forward_scheme: 'http',
      forward_host: '',
      forward_port: '80',
      websockets_upgrade: false,
      block_exploits: false,
      cache_assets: false,
      http2: true,
      force_ssl: false,
      hsts: false,
      hsts_subdomains: false,
      trust_forwarded_proto: false,
      certificate_id: null,
      locations: [],
      advanced_snippet: '',
    }
  }
  return {
    domains: [...host.domains],
    forward_scheme: host.forward_scheme,
    forward_host: host.forward_host,
    forward_port: String(host.forward_port),
    websockets_upgrade: host.websockets_upgrade,
    block_exploits: host.block_exploits,
    cache_assets: host.cache_assets,
    http2: host.http2,
    force_ssl: host.force_ssl,
    hsts: host.hsts,
    hsts_subdomains: host.hsts_subdomains,
    trust_forwarded_proto: host.trust_forwarded_proto,
    certificate_id: host.certificate_id,
    locations: host.locations.map((location) => ({
      path: location.path,
      forward_scheme: location.forward_scheme,
      forward_host: location.forward_host,
      forward_port: String(location.forward_port),
      rewrite: location.rewrite ?? '',
    })),
    advanced_snippet: host.advanced_snippet ?? '',
  }
}

interface HostEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The host being edited, or null when creating a new one. */
  host: Host | null
}

export function HostEditorDialog({
  open,
  onOpenChange,
  host,
}: HostEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {host === null ? t('hosts.editor.createTitle') : t('hosts.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>{t('hosts.editor.description')}</DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target host changes so state resets. */}
        <HostEditorForm
          key={host?.id ?? 'new'}
          host={host}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface HostEditorFormProps {
  host: Host | null
  onDone: () => void
}

export function HostEditorForm({ host, onDone }: HostEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  // Kept fresh via the shared ['certificates'] key so newly created certs appear.
  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  const certificates = certsQuery.data?.certificates ?? []

  const [form, setForm] = useState<FormState>(() => initialState(host))
  const [domainDraft, setDomainDraft] = useState('')
  const [domainError, setDomainError] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [tab, setTab] = useState('details')

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const mutation = useMutation({
    mutationFn: (input: HostInput) =>
      host === null ? api.createHost(input) : api.updateHost(host.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['hosts'] })
      toast({
        title: t('hosts.unappliedTitle'),
        description: t('hosts.unappliedBody'),
      })
      onDone()
    },
  })

  const serverError = useMemo(() => {
    if (!mutation.isError) {
      return null
    }
    if (mutation.error instanceof ApiError) {
      return { code: mutation.error.code, message: mutation.error.message }
    }
    return { code: 'unknown_error', message: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const addDomain = () => {
    const candidate = domainDraft.trim().toLowerCase()
    if (candidate === '') {
      return
    }
    if (!isValidDomain(candidate)) {
      setDomainError(t('hosts.editor.invalidDomain'))
      return
    }
    if (form.domains.includes(candidate)) {
      setDomainError(t('hosts.editor.duplicateDomain'))
      return
    }
    patch({ domains: [...form.domains, candidate] })
    setDomainDraft('')
    setDomainError(null)
  }

  const removeDomain = (domain: string) =>
    patch({ domains: form.domains.filter((item) => item !== domain) })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setFormError(null)

    if (form.domains.length === 0) {
      setFormError(t('hosts.editor.noDomains'))
      setTab('details')
      return
    }
    if (form.forward_host.trim() === '') {
      setFormError(t('hosts.editor.noForwardHost'))
      setTab('details')
      return
    }
    const port = Number.parseInt(form.forward_port, 10)
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      setFormError(t('hosts.editor.invalidPort'))
      setTab('details')
      return
    }

    const locations = form.locations.map((location) => ({
      path: location.path.trim(),
      forward_scheme: location.forward_scheme,
      forward_host: location.forward_host.trim(),
      forward_port: Number.parseInt(location.forward_port, 10) || 0,
      rewrite: location.rewrite.trim() === '' ? null : location.rewrite.trim(),
    }))

    const input: HostInput = {
      domains: form.domains,
      forward_scheme: form.forward_scheme,
      forward_host: form.forward_host.trim(),
      forward_port: port,
      websockets_upgrade: form.websockets_upgrade,
      block_exploits: form.block_exploits,
      cache_assets: form.cache_assets,
      http2: form.http2,
      force_ssl: form.force_ssl,
      hsts: form.hsts,
      hsts_subdomains: form.hsts_subdomains,
      trust_forwarded_proto: form.trust_forwarded_proto,
      certificate_id: form.certificate_id,
      locations,
      advanced_snippet:
        form.advanced_snippet.trim() === '' ? null : form.advanced_snippet,
      enabled: host === null ? true : host.enabled,
    }

    mutation.mutate(input)
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="details">{t('hosts.editor.tabs.details')}</TabsTrigger>
          <TabsTrigger value="ssl">{t('hosts.editor.tabs.ssl')}</TabsTrigger>
          <TabsTrigger value="locations">
            {t('hosts.editor.tabs.locations')}
          </TabsTrigger>
          <TabsTrigger value="advanced">
            {t('hosts.editor.tabs.advanced')}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="details" className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="host-domain-input">{t('hosts.editor.domains')}</Label>
            <div className="flex flex-wrap gap-1.5">
              {form.domains.map((domain) => (
                <span
                  key={domain}
                  className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-sm"
                >
                  {domain}
                  <button
                    type="button"
                    onClick={() => removeDomain(domain)}
                    className="text-muted-foreground hover:text-foreground"
                    aria-label={t('hosts.editor.removeDomain', { domain })}
                  >
                    <X className="size-3" aria-hidden="true" />
                  </button>
                </span>
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                id="host-domain-input"
                value={domainDraft}
                placeholder="example.com"
                onChange={(event) => {
                  setDomainDraft(event.target.value)
                  setDomainError(null)
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ',') {
                    event.preventDefault()
                    addDomain()
                  }
                }}
              />
              <Button type="button" variant="outline" onClick={addDomain}>
                <Plus aria-hidden="true" />
                {t('hosts.editor.addDomain')}
              </Button>
            </div>
            {domainError !== null && (
              <p role="alert" className="text-sm text-destructive">
                {domainError}
              </p>
            )}
          </div>

          <div className="grid gap-4 sm:grid-cols-[8rem_1fr_7rem]">
            <div className="space-y-2">
              <Label htmlFor="host-scheme">{t('hosts.editor.forwardScheme')}</Label>
              <Select
                value={form.forward_scheme}
                onValueChange={(value) =>
                  patch({ forward_scheme: value as ForwardScheme })
                }
              >
                <SelectTrigger id="host-scheme">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="http">http</SelectItem>
                  <SelectItem value="https">https</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-forward-host">{t('hosts.editor.forwardHost')}</Label>
              <Input
                id="host-forward-host"
                value={form.forward_host}
                placeholder="10.0.0.5"
                onChange={(event) => patch({ forward_host: event.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-forward-port">{t('hosts.editor.forwardPort')}</Label>
              <Input
                id="host-forward-port"
                type="number"
                min={1}
                max={65535}
                value={form.forward_port}
                onChange={(event) => patch({ forward_port: event.target.value })}
              />
            </div>
          </div>

          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-websockets"
              label={t('hosts.editor.websockets')}
              checked={form.websockets_upgrade}
              onChange={(checked) => patch({ websockets_upgrade: checked })}
            />
            <ToggleRow
              id="host-block-exploits"
              label={t('hosts.editor.blockExploits')}
              checked={form.block_exploits}
              onChange={(checked) => patch({ block_exploits: checked })}
            />
            <ToggleRow
              id="host-cache-assets"
              label={t('hosts.editor.cacheAssets')}
              checked={form.cache_assets}
              onChange={(checked) => patch({ cache_assets: checked })}
            />
          </div>
        </TabsContent>

        <TabsContent value="ssl" className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="host-certificate">
              {t('hosts.editor.ssl.certificate')}
            </Label>
            <Select
              value={
                form.certificate_id === null ? 'none' : String(form.certificate_id)
              }
              onValueChange={(value) =>
                patch({
                  certificate_id: value === 'none' ? null : Number.parseInt(value, 10),
                })
              }
            >
              <SelectTrigger id="host-certificate">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">
                  {t('hosts.editor.ssl.certificateNone')}
                </SelectItem>
                {certificates.map((cert) => (
                  <SelectItem key={cert.id} value={String(cert.id)}>
                    {cert.name} — {cert.domains.join(', ')}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {certsQuery.isError && (
              <p role="alert" className="text-sm text-destructive">
                {t('hosts.editor.ssl.loadFailed')}
              </p>
            )}
            <p className="text-sm text-muted-foreground">
              {form.certificate_id === null
                ? t('hosts.editor.ssl.selectNote')
                : t('hosts.editor.ssl.activeNote')}
            </p>
          </div>
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-force-ssl"
              label={t('hosts.editor.forceSsl')}
              checked={form.force_ssl}
              onChange={(checked) => patch({ force_ssl: checked })}
            />
            <ToggleRow
              id="host-http2"
              label={t('hosts.editor.http2')}
              checked={form.http2}
              onChange={(checked) => patch({ http2: checked })}
            />
            <ToggleRow
              id="host-hsts"
              label={t('hosts.editor.hsts')}
              checked={form.hsts}
              onChange={(checked) => patch({ hsts: checked })}
            />
            <ToggleRow
              id="host-hsts-subdomains"
              label={t('hosts.editor.hstsSubdomains')}
              checked={form.hsts_subdomains}
              onChange={(checked) => patch({ hsts_subdomains: checked })}
            />
            <ToggleRow
              id="host-trust-proto"
              label={t('hosts.editor.trustForwardedProto')}
              checked={form.trust_forwarded_proto}
              onChange={(checked) => patch({ trust_forwarded_proto: checked })}
            />
          </div>
        </TabsContent>

        <TabsContent value="locations" className="space-y-4">
          {form.locations.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {t('hosts.editor.locations.empty')}
            </p>
          )}
          {form.locations.map((location, index) => (
            <div
              key={index}
              className="space-y-3 rounded-lg border p-3"
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">
                  {t('hosts.editor.locations.item', { index: index + 1 })}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  onClick={() =>
                    patch({
                      locations: form.locations.filter((_, i) => i !== index),
                    })
                  }
                  aria-label={t('hosts.editor.locations.remove')}
                >
                  <X aria-hidden="true" />
                </Button>
              </div>
              <div className="space-y-2">
                <Label htmlFor={`loc-path-${index}`}>
                  {t('hosts.editor.locations.path')}
                </Label>
                <Input
                  id={`loc-path-${index}`}
                  value={location.path}
                  placeholder="/api"
                  onChange={(event) =>
                    patch({
                      locations: form.locations.map((item, i) =>
                        i === index ? { ...item, path: event.target.value } : item,
                      ),
                    })
                  }
                />
              </div>
              <div className="grid gap-3 sm:grid-cols-[8rem_1fr_7rem]">
                <div className="space-y-2">
                  <Label htmlFor={`loc-scheme-${index}`}>
                    {t('hosts.editor.forwardScheme')}
                  </Label>
                  <Select
                    value={location.forward_scheme}
                    onValueChange={(value) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_scheme: value as ForwardScheme }
                            : item,
                        ),
                      })
                    }
                  >
                    <SelectTrigger id={`loc-scheme-${index}`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="http">http</SelectItem>
                      <SelectItem value="https">https</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`loc-host-${index}`}>
                    {t('hosts.editor.forwardHost')}
                  </Label>
                  <Input
                    id={`loc-host-${index}`}
                    value={location.forward_host}
                    onChange={(event) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_host: event.target.value }
                            : item,
                        ),
                      })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`loc-port-${index}`}>
                    {t('hosts.editor.forwardPort')}
                  </Label>
                  <Input
                    id={`loc-port-${index}`}
                    type="number"
                    min={1}
                    max={65535}
                    value={location.forward_port}
                    onChange={(event) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_port: event.target.value }
                            : item,
                        ),
                      })
                    }
                  />
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor={`loc-rewrite-${index}`}>
                  {t('hosts.editor.locations.rewrite')}
                </Label>
                <Input
                  id={`loc-rewrite-${index}`}
                  value={location.rewrite}
                  placeholder="/ /$1 break"
                  onChange={(event) =>
                    patch({
                      locations: form.locations.map((item, i) =>
                        i === index ? { ...item, rewrite: event.target.value } : item,
                      ),
                    })
                  }
                />
              </div>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              patch({
                locations: [
                  ...form.locations,
                  {
                    path: '',
                    forward_scheme: 'http',
                    forward_host: '',
                    forward_port: '80',
                    rewrite: '',
                  },
                ],
              })
            }
          >
            <Plus aria-hidden="true" />
            {t('hosts.editor.locations.add')}
          </Button>
        </TabsContent>

        <TabsContent value="advanced" className="space-y-4">
          <Alert variant="destructive">
            <AlertTitle>{t('hosts.editor.advanced.warningTitle')}</AlertTitle>
            <AlertDescription>
              {t('hosts.editor.advanced.warningBody')}
            </AlertDescription>
          </Alert>
          <div className="space-y-2">
            <Label htmlFor="host-advanced-snippet">
              {t('hosts.editor.advanced.label')}
            </Label>
            <Textarea
              id="host-advanced-snippet"
              className="min-h-40 font-mono text-xs"
              value={form.advanced_snippet}
              spellCheck={false}
              onChange={(event) => patch({ advanced_snippet: event.target.value })}
            />
          </div>
        </TabsContent>
      </Tabs>

      {formError !== null && (
        <p role="alert" className="text-sm text-destructive">
          {formError}
        </p>
      )}
      {serverError !== null && (
        <Alert variant="destructive">
          <AlertTitle>{t('hosts.editor.saveFailed')}</AlertTitle>
          <AlertDescription>{serverError.message}</AlertDescription>
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
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

interface ToggleRowProps {
  id: string
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}

function ToggleRow({ id, label, checked, onChange }: ToggleRowProps) {
  return (
    <div className="flex items-center justify-between gap-4">
      <Label htmlFor={id} className="font-normal">
        {label}
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  )
}

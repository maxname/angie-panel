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
  type BalanceMethod,
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

interface ServerDraft {
  host: string
  port: string
  weight: string
  backup: boolean
  down: boolean
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
  access_list_id: number | null
  locations: LocationDraft[]
  advanced_snippet: string
  rate_limit_enabled: boolean
  rate_limit_rps: string
  rate_limit_burst: string
  rate_limit_nodelay: boolean
  rate_limit_conn: string
  balance_method: BalanceMethod
  primary_weight: string
  max_fails: string
  fail_timeout_secs: string
  servers: ServerDraft[]
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
      access_list_id: null,
      locations: [],
      advanced_snippet: '',
      rate_limit_enabled: false,
      rate_limit_rps: '',
      rate_limit_burst: '',
      rate_limit_nodelay: false,
      rate_limit_conn: '',
      balance_method: 'round_robin',
      primary_weight: '1',
      max_fails: '1',
      fail_timeout_secs: '10',
      servers: [],
    }
  }
  const rl = host.rate_limit
  const up = host.upstream
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
    access_list_id: host.access_list_id,
    locations: host.locations.map((location) => ({
      path: location.path,
      forward_scheme: location.forward_scheme,
      forward_host: location.forward_host,
      forward_port: String(location.forward_port),
      rewrite: location.rewrite ?? '',
    })),
    advanced_snippet: host.advanced_snippet ?? '',
    rate_limit_enabled: rl.enabled,
    rate_limit_rps: rl.rps > 0 ? String(rl.rps) : '',
    rate_limit_burst: rl.burst > 0 ? String(rl.burst) : '',
    rate_limit_nodelay: rl.nodelay,
    rate_limit_conn: rl.conn > 0 ? String(rl.conn) : '',
    balance_method: up.method,
    primary_weight: String(up.primary_weight),
    max_fails: String(up.max_fails),
    fail_timeout_secs: String(up.fail_timeout_secs),
    servers: up.servers.map((s) => ({
      host: s.host,
      port: String(s.port),
      weight: String(s.weight),
      backup: s.backup,
      down: s.down,
    })),
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

  // Shares the ['access-lists'] key so lists created on that page appear here.
  const accessListsQuery = useQuery({
    queryKey: ['access-lists'],
    queryFn: () => api.listAccessLists(),
  })
  const accessLists = accessListsQuery.data?.access_lists ?? []

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
    const rlRps = Number.parseInt(form.rate_limit_rps, 10) || 0
    const rlConn = Number.parseInt(form.rate_limit_conn, 10) || 0
    if (form.rate_limit_enabled && rlRps <= 0 && rlConn <= 0) {
      setFormError(t('hosts.editor.rateLimit.errNoLimit'))
      setTab('rateLimit')
      return
    }

    // Additional upstream servers: each needs a host and a valid port; ip_hash
    // forbids backup peers (Angie rejects the combo).
    for (const s of form.servers) {
      const sp = Number.parseInt(s.port, 10)
      if (s.host.trim() === '' || !Number.isInteger(sp) || sp < 1 || sp > 65535) {
        setFormError(t('hosts.editor.upstreams.errServer'))
        setTab('upstreams')
        return
      }
      if (form.balance_method === 'ip_hash' && s.backup) {
        setFormError(t('hosts.editor.upstreams.errIpHashBackup'))
        setTab('upstreams')
        return
      }
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
      access_list_id: form.access_list_id,
      locations,
      advanced_snippet:
        form.advanced_snippet.trim() === '' ? null : form.advanced_snippet,
      rate_limit: {
        enabled: form.rate_limit_enabled,
        rps: rlRps,
        burst: Number.parseInt(form.rate_limit_burst, 10) || 0,
        nodelay: form.rate_limit_nodelay,
        conn: rlConn,
      },
      upstream: {
        method: form.balance_method,
        primary_weight: Number.parseInt(form.primary_weight, 10) || 1,
        max_fails: Number.parseInt(form.max_fails, 10) || 0,
        fail_timeout_secs: Number.parseInt(form.fail_timeout_secs, 10) || 10,
        servers: form.servers.map((s) => ({
          host: s.host.trim(),
          port: Number.parseInt(s.port, 10) || 0,
          weight: Number.parseInt(s.weight, 10) || 1,
          backup: s.backup,
          down: s.down,
        })),
      },
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
          <TabsTrigger value="upstreams">
            {t('hosts.editor.tabs.upstreams')}
          </TabsTrigger>
          <TabsTrigger value="rateLimit">
            {t('hosts.editor.tabs.rateLimit')}
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

          <div className="space-y-2">
            <Label htmlFor="host-access-list">
              {t('hosts.editor.accessList')}
            </Label>
            <Select
              value={
                form.access_list_id === null ? 'none' : String(form.access_list_id)
              }
              onValueChange={(value) =>
                patch({
                  access_list_id:
                    value === 'none' ? null : Number.parseInt(value, 10),
                })
              }
            >
              <SelectTrigger id="host-access-list">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">
                  {t('hosts.editor.accessListNone')}
                </SelectItem>
                {accessLists.map((list) => (
                  <SelectItem key={list.id} value={String(list.id)}>
                    {list.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {accessListsQuery.isError && (
              <p role="alert" className="text-sm text-destructive">
                {t('hosts.editor.accessListLoadFailed')}
              </p>
            )}
            <p className="text-sm text-muted-foreground">
              {t('hosts.editor.accessListHelp')}
            </p>
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

        <TabsContent value="upstreams" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('hosts.editor.upstreams.description')}
          </p>

          <div className="grid gap-4 sm:grid-cols-[1fr_8rem]">
            <div className="space-y-2">
              <Label htmlFor="host-balance-method">
                {t('hosts.editor.upstreams.method')}
              </Label>
              <Select
                value={form.balance_method}
                onValueChange={(value) =>
                  patch({ balance_method: value as BalanceMethod })
                }
              >
                <SelectTrigger id="host-balance-method">
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

          {/* Server pool: the primary (from Details) plus additional peers. */}
          <div className="space-y-3 rounded-lg border p-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">
                {t('hosts.editor.upstreams.servers')}
              </span>
            </div>
            <div className="flex items-center gap-2 text-sm">
              <span className="rounded bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                {t('hosts.editor.upstreams.primary')}
              </span>
              <span className="font-mono text-xs">
                {form.forward_host || '—'}:{form.forward_port || '—'}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <Label
                  htmlFor="host-primary-weight"
                  className="text-xs font-normal text-muted-foreground"
                >
                  {t('hosts.editor.upstreams.weight')}
                </Label>
                <Input
                  id="host-primary-weight"
                  inputMode="numeric"
                  className="h-8 w-16"
                  value={form.primary_weight}
                  onChange={(event) =>
                    patch({
                      primary_weight: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
            </div>

            {form.servers.map((server, index) => (
              <div
                key={index}
                className="flex flex-wrap items-center gap-2 border-t pt-3"
              >
                <Input
                  aria-label={t('hosts.editor.upstreams.serverHost')}
                  placeholder="10.0.0.2"
                  className="h-8 w-40"
                  value={server.host}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index ? { ...s, host: event.target.value } : s,
                      ),
                    })
                  }
                />
                <Input
                  aria-label={t('hosts.editor.upstreams.serverPort')}
                  inputMode="numeric"
                  placeholder="8080"
                  className="h-8 w-20"
                  value={server.port}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index
                          ? { ...s, port: event.target.value.replace(/[^0-9]/g, '') }
                          : s,
                      ),
                    })
                  }
                />
                <Input
                  aria-label={t('hosts.editor.upstreams.weight')}
                  inputMode="numeric"
                  placeholder="1"
                  className="h-8 w-14"
                  value={server.weight}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index
                          ? { ...s, weight: event.target.value.replace(/[^0-9]/g, '') }
                          : s,
                      ),
                    })
                  }
                />
                <label className="flex items-center gap-1 text-xs">
                  <input
                    type="checkbox"
                    checked={server.backup}
                    onChange={(event) =>
                      patch({
                        servers: form.servers.map((s, i) =>
                          i === index ? { ...s, backup: event.target.checked } : s,
                        ),
                      })
                    }
                  />
                  {t('hosts.editor.upstreams.backup')}
                </label>
                <label className="flex items-center gap-1 text-xs">
                  <input
                    type="checkbox"
                    checked={server.down}
                    onChange={(event) =>
                      patch({
                        servers: form.servers.map((s, i) =>
                          i === index ? { ...s, down: event.target.checked } : s,
                        ),
                      })
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
                    patch({
                      servers: form.servers.filter((_, i) => i !== index),
                    })
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
                patch({
                  servers: [
                    ...form.servers,
                    { host: '', port: '', weight: '1', backup: false, down: false },
                  ],
                })
              }
            >
              <Plus aria-hidden="true" />
              {t('hosts.editor.upstreams.addServer')}
            </Button>
          </div>

          {/* Passive health checks. */}
          <div className="space-y-3 rounded-lg border p-3">
            <span className="text-sm font-medium">
              {t('hosts.editor.upstreams.health')}
            </span>
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="host-max-fails">
                  {t('hosts.editor.upstreams.maxFails')}
                </Label>
                <Input
                  id="host-max-fails"
                  inputMode="numeric"
                  value={form.max_fails}
                  onChange={(event) =>
                    patch({ max_fails: event.target.value.replace(/[^0-9]/g, '') })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-fail-timeout">
                  {t('hosts.editor.upstreams.failTimeout')}
                </Label>
                <Input
                  id="host-fail-timeout"
                  inputMode="numeric"
                  value={form.fail_timeout_secs}
                  onChange={(event) =>
                    patch({
                      fail_timeout_secs: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.upstreams.healthHint')}
            </p>
          </div>
        </TabsContent>

        <TabsContent value="rateLimit" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('hosts.editor.rateLimit.description')}
          </p>
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-rate-limit-enabled"
              label={t('hosts.editor.rateLimit.enable')}
              checked={form.rate_limit_enabled}
              onChange={(checked) => patch({ rate_limit_enabled: checked })}
            />
          </div>
          {form.rate_limit_enabled && (
            <div className="space-y-4 rounded-lg border p-3">
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="host-rl-rps">
                    {t('hosts.editor.rateLimit.rps')}
                  </Label>
                  <Input
                    id="host-rl-rps"
                    inputMode="numeric"
                    placeholder="10"
                    value={form.rate_limit_rps}
                    onChange={(event) =>
                      patch({
                        rate_limit_rps: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="host-rl-burst">
                    {t('hosts.editor.rateLimit.burst')}
                  </Label>
                  <Input
                    id="host-rl-burst"
                    inputMode="numeric"
                    placeholder="20"
                    value={form.rate_limit_burst}
                    onChange={(event) =>
                      patch({
                        rate_limit_burst: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                </div>
              </div>
              <ToggleRow
                id="host-rl-nodelay"
                label={t('hosts.editor.rateLimit.nodelay')}
                checked={form.rate_limit_nodelay}
                onChange={(checked) => patch({ rate_limit_nodelay: checked })}
              />
              <div className="space-y-2 sm:max-w-[16rem]">
                <Label htmlFor="host-rl-conn">
                  {t('hosts.editor.rateLimit.conn')}
                </Label>
                <Input
                  id="host-rl-conn"
                  inputMode="numeric"
                  placeholder="0"
                  value={form.rate_limit_conn}
                  onChange={(event) =>
                    patch({
                      rate_limit_conn: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t('hosts.editor.rateLimit.hint')}
              </p>
            </div>
          )}
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

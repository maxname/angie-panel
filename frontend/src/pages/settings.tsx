import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Loader2 } from 'lucide-react'
import { useRef, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
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
import {
  api,
  ApiError,
  type DefaultSite,
  type ImportResult,
  type SettingsResponse,
} from '@/lib/api'
import { toast } from '@/lib/toast'

const DEFAULT_SITE_OPTIONS: DefaultSite[] = [
  'notfound',
  'drop444',
  'redirect',
  'html',
]

export function SettingsPage() {
  const { t } = useTranslation()

  const settingsQuery = useQuery({
    queryKey: ['settings'],
    queryFn: () => api.getSettings(),
  })

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">{t('settings.title')}</h1>
      {settingsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : settingsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('settings.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void settingsQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <>
          <SettingsForm data={settingsQuery.data} />
          <DnsProviders />
        </>
      )}
      <BackupRestore />
    </div>
  )
}

/** DNS-01 provider credentials for automatic wildcard issuance. Pick a provider,
 *  enter its API credentials (write-only — the panel never returns them, only a
 *  "configured" flag). Backed by acme.sh's dnsapi plugins. */
function DnsProviders() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const providersQuery = useQuery({
    queryKey: ['dns-providers'],
    queryFn: () => api.listDnsProviders(),
  })
  const providers = providersQuery.data?.providers ?? []
  const [selectedId, setSelectedId] = useState<string>('')
  const [values, setValues] = useState<Record<string, string>>({})

  const selected = providers.find((p) => p.id === selectedId) ?? providers[0]

  const saveMutation = useMutation({
    mutationFn: (creds: Record<string, string>) =>
      api.setDnsProviderCredentials(selected!.id, creds),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dns-providers'] })
      setValues({})
      toast({ variant: 'success', title: t('settings.saved') })
    },
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!selected) return
    const creds: Record<string, string> = {}
    for (const field of selected.fields) {
      creds[field.env] = (values[field.env] ?? '').trim()
    }
    saveMutation.mutate(creds)
  }

  const disconnect = () => {
    if (!selected) return
    const creds: Record<string, string> = {}
    for (const field of selected.fields) creds[field.env] = ''
    saveMutation.mutate(creds)
  }

  const error =
    saveMutation.isError && saveMutation.error instanceof ApiError
      ? saveMutation.error.message
      : saveMutation.isError
        ? t('common.error')
        : null

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('settings.dnsProviders.title')}</CardTitle>
        <CardDescription>
          {t('settings.dnsProviders.description')}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {providersQuery.isPending ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" aria-hidden="true" />
            {t('common.loading')}
          </div>
        ) : selected === undefined ? (
          <p className="text-sm text-muted-foreground">
            {t('settings.dnsProviders.none')}
          </p>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="grid gap-2 sm:max-w-xs">
              <Label htmlFor="dns-provider">
                {t('settings.dnsProviders.provider')}
              </Label>
              <Select
                value={selected.id}
                onValueChange={(value) => {
                  setSelectedId(value)
                  setValues({})
                }}
              >
                <SelectTrigger id="dns-provider">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {providers.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.label}
                      {p.configured
                        ? ` — ${t('settings.dnsProviders.configured')}`
                        : ''}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            {selected.fields.map((field) => (
              <div key={field.env} className="grid gap-2 sm:max-w-xs">
                <Label htmlFor={`dns-cred-${field.env}`}>{field.label}</Label>
                <Input
                  id={`dns-cred-${field.env}`}
                  type="password"
                  autoComplete="new-password"
                  placeholder={selected.configured ? '••••••••' : ''}
                  value={values[field.env] ?? ''}
                  onChange={(e) =>
                    setValues((v) => ({ ...v, [field.env]: e.target.value }))
                  }
                />
              </div>
            ))}
            <p className="text-xs text-muted-foreground">
              {t('settings.dnsProviders.hint')}
            </p>
            {error !== null && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
            <div className="flex items-center gap-2">
              <Button
                type="submit"
                disabled={
                  saveMutation.isPending ||
                  selected.fields.some((f) => (values[f.env] ?? '').trim() === '')
                }
              >
                {saveMutation.isPending && (
                  <Loader2 className="animate-spin" aria-hidden="true" />
                )}
                {t('settings.dnsProviders.save')}
              </Button>
              {selected.configured && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={saveMutation.isPending}
                  onClick={disconnect}
                >
                  {t('settings.dnsProviders.disconnect')}
                </Button>
              )}
            </div>
          </form>
        )}
      </CardContent>
    </Card>
  )
}

function BackupRestore() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [pendingImport, setPendingImport] = useState<unknown | null>(null)
  const [parseError, setParseError] = useState<string | null>(null)

  const exportMutation = useMutation({
    mutationFn: () => api.exportConfig(),
    onSuccess: (doc) => {
      // Trigger a client-side download of the returned JSON.
      const date = new Date().toISOString().slice(0, 10)
      const blob = new Blob([JSON.stringify(doc, null, 2)], {
        type: 'application/json',
      })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `angie-panel-config-${date}.json`
      document.body.appendChild(a)
      a.click()
      a.remove()
      URL.revokeObjectURL(url)
      toast({ variant: 'success', title: t('settings.backup.exported') })
    },
    onError: () =>
      toast({ variant: 'destructive', title: t('settings.backup.exportFailed') }),
  })

  const importMutation = useMutation({
    mutationFn: (doc: unknown) => api.importConfig(doc),
    onSuccess: (result: ImportResult) => {
      setPendingImport(null)
      for (const key of [
        'hosts',
        'redirect-hosts',
        'dead-hosts',
        'streams',
        'certificates',
        'access-lists',
        'dashboard',
      ]) {
        void queryClient.invalidateQueries({ queryKey: [key] })
      }
      const i = result.imported
      toast({
        variant: 'success',
        title: t('settings.backup.imported', {
          hosts: i.hosts + i.redirect_hosts + i.dead_hosts,
          streams: i.streams,
          certs: i.certificates,
          lists: i.access_lists,
        }),
      })
    },
  })

  const onFilePicked = (event: React.ChangeEvent<HTMLInputElement>) => {
    setParseError(null)
    const file = event.target.files?.[0]
    // Reset so picking the same file again re-fires onChange.
    event.target.value = ''
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      try {
        setPendingImport(JSON.parse(String(reader.result)))
      } catch {
        setParseError(t('settings.backup.notJson'))
      }
    }
    reader.onerror = () => setParseError(t('settings.backup.notJson'))
    reader.readAsText(file)
  }

  const importError =
    importMutation.isError && importMutation.error instanceof ApiError
      ? importMutation.error.message
      : importMutation.isError
        ? t('common.error')
        : null

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('settings.backup.title')}</CardTitle>
        <CardDescription>{t('settings.backup.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-3">
          <Button
            type="button"
            variant="outline"
            onClick={() => exportMutation.mutate()}
            disabled={exportMutation.isPending}
          >
            {exportMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('settings.backup.exportButton')}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => fileInputRef.current?.click()}
          >
            {t('settings.backup.importButton')}
          </Button>
          <input
            ref={fileInputRef}
            type="file"
            accept="application/json,.json"
            className="hidden"
            onChange={onFilePicked}
          />
        </div>
        <p className="text-sm text-muted-foreground">
          {t('settings.backup.secretsNote')}
        </p>
        {parseError !== null && (
          <p className="text-sm text-destructive" role="alert">
            {parseError}
          </p>
        )}
      </CardContent>

      <Dialog
        open={pendingImport !== null}
        onOpenChange={(open) => {
          if (!open) setPendingImport(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('settings.backup.confirmTitle')}</DialogTitle>
            <DialogDescription>
              {t('settings.backup.confirmBody')}
            </DialogDescription>
          </DialogHeader>
          {importError !== null && (
            <Alert variant="destructive">
              <AlertDescription>{importError}</AlertDescription>
            </Alert>
          )}
          {importMutation.isSuccess ? (
            <Alert>
              <AlertDescription>
                <Link to="/apply" className="font-medium underline underline-offset-4">
                  {t('settings.backup.goToApply')}
                </Link>
              </AlertDescription>
            </Alert>
          ) : null}
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setPendingImport(null)}
            >
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={importMutation.isPending}
              onClick={() => {
                if (pendingImport !== null) importMutation.mutate(pendingImport)
              }}
            >
              {importMutation.isPending && (
                <Loader2 className="animate-spin" aria-hidden="true" />
              )}
              {t('settings.backup.confirmImport')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  )
}

function SettingsForm({ data }: { data: SettingsResponse }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const [defaultSite, setDefaultSite] = useState<string>(
    data.raw.default_site ?? 'notfound',
  )
  const [redirectUrl, setRedirectUrl] = useState<string>(
    data.raw.default_site_redirect_url ?? '',
  )
  const [ipv6Enabled, setIpv6Enabled] = useState<boolean>(
    data.raw.ipv6_enabled === '1',
  )
  const [resolverOverride, setResolverOverride] = useState<string>(
    data.raw.resolver_override ?? '',
  )
  const [acmeEmail, setAcmeEmail] = useState<string>(data.raw.acme_email ?? '')

  const mutation = useMutation({
    mutationFn: (body: Record<string, string>) => api.updateSettings(body),
    onSuccess: (result) => {
      queryClient.setQueryData(['settings'], result)
      toast({ variant: 'success', title: t('settings.saved') })
    },
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    mutation.mutate({
      default_site: defaultSite,
      default_site_redirect_url: redirectUrl,
      ipv6_enabled: ipv6Enabled ? '1' : '0',
      resolver_override: resolverOverride,
      acme_email: acmeEmail,
    })
  }

  const serverError =
    mutation.isError && mutation.error instanceof ApiError
      ? mutation.error.message
      : mutation.isError
        ? t('common.error')
        : null

  return (
    <form onSubmit={handleSubmit} className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.defaultSite.title')}</CardTitle>
          <CardDescription>{t('settings.defaultSite.description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-2 sm:max-w-xs">
            <Label htmlFor="default-site">{t('settings.defaultSite.label')}</Label>
            <Select value={defaultSite} onValueChange={setDefaultSite}>
              <SelectTrigger id="default-site">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {DEFAULT_SITE_OPTIONS.map((option) => (
                  <SelectItem key={option} value={option}>
                    {t(`settings.defaultSite.options.${option}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {defaultSite === 'redirect' && (
            <div className="grid gap-2 sm:max-w-md">
              <Label htmlFor="redirect-url">
                {t('settings.defaultSite.redirectUrl')}
              </Label>
              <Input
                id="redirect-url"
                type="url"
                inputMode="url"
                placeholder="https://example.com"
                value={redirectUrl}
                onChange={(event) => setRedirectUrl(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t('settings.defaultSite.redirectHelp')}
              </p>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.network.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between gap-4">
            <div className="space-y-1">
              <Label htmlFor="ipv6">{t('settings.network.ipv6')}</Label>
              <p className="text-xs text-muted-foreground">
                {t('settings.network.ipv6Effective', {
                  value: data.effective.ipv6_enabled
                    ? t('common.yes')
                    : t('common.no'),
                })}
              </p>
            </div>
            <Switch id="ipv6" checked={ipv6Enabled} onCheckedChange={setIpv6Enabled} />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="resolver">{t('settings.network.resolver')}</Label>
            <Input
              id="resolver"
              placeholder="1.1.1.1 8.8.8.8"
              value={resolverOverride}
              onChange={(event) => setResolverOverride(event.target.value)}
            />
            <p className="text-xs text-muted-foreground">
              {t('settings.network.resolverEffective', {
                value:
                  data.effective.resolvers.length > 0
                    ? data.effective.resolvers.join(', ')
                    : t('settings.network.resolverNone'),
              })}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.acme.title')}</CardTitle>
          <CardDescription>{t('settings.acme.description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-2 sm:max-w-md">
            <Label htmlFor="acme-email">{t('settings.acme.email')}</Label>
            <Input
              id="acme-email"
              type="email"
              inputMode="email"
              placeholder="admin@example.com"
              value={acmeEmail}
              onChange={(event) => setAcmeEmail(event.target.value)}
            />
          </div>
        </CardContent>
      </Card>

      {serverError !== null && (
        <Alert variant="destructive">
          <AlertTitle>{t('settings.saveFailed')}</AlertTitle>
          <AlertDescription>{serverError}</AlertDescription>
        </Alert>
      )}

      <div className="flex justify-end">
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('common.save')}
        </Button>
      </div>
    </form>
  )
}

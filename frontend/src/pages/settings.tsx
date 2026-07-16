import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Loader2 } from 'lucide-react'
import { useMemo, useRef, useState, type FormEvent } from 'react'
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
import { ChipsField } from '@/components/chips-field'
import { DefaultSitePicker } from '@/components/default-site-picker'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import {
  api,
  ApiError,
  type ImportResult,
  type SettingsResponse,
} from '@/lib/api'
import { isValidIp } from '@/lib/ip'
import { toast } from '@/lib/toast'
import { useIsDirty } from '@/lib/use-dirty'


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
        <SettingsForm data={settingsQuery.data} />
      )}
      <BackupRestore />
    </div>
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
      // Keep the dialog open so the success alert + "go to Apply" link render
      // (closing here made that block unreachable — the import creates pending
      // changes the user must apply).
      // Import replaces the ENTIRE config, so invalidate every dependent query
      // — including bans/settings/geo/sni-routers, which the import also touches
      // and which are edited from this very page.
      for (const key of [
        'hosts',
        'redirect-hosts',
        'dead-hosts',
        'streams',
        'sni-routers',
        'certificates',
        'access-lists',
        'bans',
        'settings',
        'geo',
        'dns-providers',
        'dns-credentials',
        'apply',
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
          if (!open) {
            setPendingImport(null)
            // Reset so a later import doesn't reopen showing the previous
            // success state.
            importMutation.reset()
          }
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
            {importMutation.isSuccess ? (
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  setPendingImport(null)
                  importMutation.reset()
                }}
              >
                {t('common.close')}
              </Button>
            ) : (
              <>
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
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  )
}

/** The saved settings in the shape the form edits. One place, so the initial
 *  values and the "has anything changed?" baseline can't drift apart. */
function savedValues(raw: SettingsResponse['raw']) {
  return {
    defaultSite: raw.default_site ?? 'notfound',
    redirectUrl: raw.default_site_redirect_url ?? '',
    ipv6Enabled: raw.ipv6_enabled === '1',
    // Stored as the space/comma list the backend parses; edited as a list of
    // addresses, since that is what it is.
    resolvers: (raw.resolver_override ?? '').split(/[\s,]+/).filter(Boolean),
    acmeEmail: raw.acme_email ?? '',
  }
}

function SettingsForm({ data }: { data: SettingsResponse }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  // Derived from the server's copy rather than snapshotted on mount: saving
  // updates the cache in place without remounting this form, and a mount-time
  // snapshot would leave it looking dirty forever afterwards.
  const saved = useMemo(() => savedValues(data.raw), [data])

  const [defaultSite, setDefaultSite] = useState<string>(saved.defaultSite)
  const [redirectUrl, setRedirectUrl] = useState<string>(saved.redirectUrl)
  const [ipv6Enabled, setIpv6Enabled] = useState<boolean>(saved.ipv6Enabled)
  const [resolvers, setResolvers] = useState<string[]>(saved.resolvers)
  const [acmeEmail, setAcmeEmail] = useState<string>(saved.acmeEmail)

  const isDirty = useIsDirty(
    { defaultSite, redirectUrl, ipv6Enabled, resolvers, acmeEmail },
    saved,
  )

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
      resolver_override: resolvers.join(' '),
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
          <div className="grid gap-2">
            {/* A radiogroup, not a labelled field: the label names the group
                rather than pointing at one control. */}
            <span id="default-site-label" className="text-sm font-medium">
              {t('settings.defaultSite.label')}
            </span>
            <DefaultSitePicker
              value={defaultSite}
              onChange={setDefaultSite}
              aria-labelledby="default-site-label"
            />
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

          {/* An address is short; match the other fields on this page rather
              than running a nameserver box the full width of the screen. */}
          <div className="grid gap-2 sm:max-w-md">
            <ChipsField
              id="resolver"
              label={t('settings.network.resolver')}
              values={resolvers}
              onChange={setResolvers}
              placeholder="1.1.1.1"
              addLabel={t('settings.network.addResolver')}
              removeLabel={(ip) => t('settings.network.removeResolver', { ip })}
              validate={(ip, existing) => {
                if (!isValidIp(ip)) {
                  return t('settings.network.invalidResolver')
                }
                if (existing.includes(ip)) {
                  return t('settings.network.duplicateResolver')
                }
                return null
              }}
              describedBy="resolver-help"
            />
            <p id="resolver-help" className="text-xs text-muted-foreground">
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
              autoComplete="email"
              spellCheck={false}
              autoCapitalize="off"
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
        <Button type="submit" disabled={mutation.isPending || !isDirty}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('common.save')}
        </Button>
      </div>
    </form>
  )
}

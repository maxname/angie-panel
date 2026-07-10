import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { AlertTriangle, Info, Loader2, PlayCircle } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
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
  type AlertSeverity,
  type ConfigtestReport,
  type DashboardAlert,
  type DashboardAngie,
  type DashboardCert,
  type DashboardHost,
  type DashboardUpstream,
} from '@/lib/api'

import { StatusPill } from './certificates'

const CONFIGTEST_QUERY_KEY = ['system', 'configtest'] as const

const EMERALD_BADGE =
  'bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400'

export function DashboardPage() {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{t('dashboard.title')}</h1>
        <p className="text-sm text-muted-foreground">{t('dashboard.subtitle')}</p>
      </div>
      <LiveDashboard />
      <ConfigtestSection />
    </div>
  )
}

function LiveDashboard() {
  const { t } = useTranslation()
  const query = useQuery({
    queryKey: ['dashboard'],
    queryFn: () => api.getDashboard(),
    // Live view: re-poll every 5 seconds. A 401 makes the client redirect to
    // /login, so we do not special-case auth here.
    refetchInterval: 5000,
  })

  if (query.isPending) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" aria-hidden="true" />
        {t('common.loading')}
      </div>
    )
  }

  if (query.isError) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-3">
          <p className="text-sm text-destructive" role="alert">
            {t('dashboard.loadFailed')}
          </p>
          <Button variant="outline" size="sm" onClick={() => void query.refetch()}>
            {t('common.retry')}
          </Button>
        </CardContent>
      </Card>
    )
  }

  const data = query.data

  return (
    <div className="space-y-6">
      <AlertsBanner alerts={data.alerts} foreignFiles={data.drift.foreign_files} />
      <AngieStatusCard angie={data.angie} />
      <CertificatesSection certificates={data.certificates} />
      <HostsSection hosts={data.hosts} />
    </div>
  )
}

// --- alerts ----------------------------------------------------------------

const SEVERITY_VARIANT: Record<AlertSeverity, 'destructive' | 'warning' | 'info'> = {
  error: 'destructive',
  warning: 'warning',
  info: 'info',
}

function AlertsBanner({
  alerts,
  foreignFiles,
}: {
  alerts: DashboardAlert[]
  foreignFiles: string[]
}) {
  // Empty alerts → no banner at all.
  if (alerts.length === 0) {
    return null
  }
  return (
    <div className="space-y-3">
      {alerts.map((alert, index) => (
        <DashboardAlertItem
          key={`${alert.code}-${index}`}
          alert={alert}
          // The drift alert carries the list of unmanaged files it found.
          foreignFiles={alert.code === 'drift' ? foreignFiles : []}
        />
      ))}
    </div>
  )
}

function DashboardAlertItem({
  alert,
  foreignFiles,
}: {
  alert: DashboardAlert
  foreignFiles: string[]
}) {
  const { t } = useTranslation()
  const Icon = alert.severity === 'info' ? Info : AlertTriangle

  // Drift and pending-changes alerts route the operator to the Apply page.
  const applyLinkLabel =
    alert.code === 'drift'
      ? t('dashboard.alerts.reapply')
      : alert.code === 'pending'
        ? t('dashboard.alerts.goToApply')
        : null

  // Localize the static alert codes; dynamic ones (cert_failed/cert_expired
  // carry the certificate name) fall back to the server-supplied message.
  const localizedMessage =
    alert.code === 'pending'
      ? t('dashboard.alerts.msgPending')
      : alert.code === 'drift'
        ? t('dashboard.alerts.msgDrift')
        : alert.code === 'angie_down'
          ? t('dashboard.alerts.msgAngieDown')
          : alert.message

  return (
    <Alert variant={SEVERITY_VARIANT[alert.severity]}>
      <Icon aria-hidden="true" />
      <AlertTitle>{t(`dashboard.alerts.${alert.severity}`)}</AlertTitle>
      <AlertDescription>
        {/* Escaped React node — localized for known codes, server text otherwise. */}
        <p>{localizedMessage}</p>
        {foreignFiles.length > 0 && (
          <ul className="mt-1 list-inside list-disc font-mono text-xs">
            {foreignFiles.map((name) => (
              <li key={name}>{name}</li>
            ))}
          </ul>
        )}
        {applyLinkLabel !== null && (
          <Link
            to="/apply"
            className="mt-1 font-medium text-current underline underline-offset-4"
          >
            {applyLinkLabel}
          </Link>
        )}
      </AlertDescription>
    </Alert>
  )
}

// --- Angie status ----------------------------------------------------------

function AngieStatusCard({ angie }: { angie: DashboardAngie }) {
  const { t, i18n } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('dashboard.angie.title')}</CardTitle>
        <CardAction>
          {angie.up ? (
            <Badge className={EMERALD_BADGE}>{t('dashboard.angie.running')}</Badge>
          ) : (
            <Badge variant="destructive">{t('dashboard.angie.unreachable')}</Badge>
          )}
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-x-10 gap-y-3">
          <Field label={t('dashboard.angie.version')}>
            <span className="font-mono text-sm">{angie.version ?? '—'}</span>
          </Field>
          <Field label={t('dashboard.angie.generation')}>
            <span className="font-mono text-sm">
              {angie.generation !== null
                ? formatNumber(angie.generation, i18n.language)
                : '—'}
            </span>
          </Field>
        </div>

        {angie.up ? (
          angie.connections !== null && (
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <StatTile
                label={t('dashboard.angie.connections.active')}
                value={angie.connections.active}
              />
              <StatTile
                label={t('dashboard.angie.connections.idle')}
                value={angie.connections.idle}
              />
              <StatTile
                label={t('dashboard.angie.connections.accepted')}
                value={angie.connections.accepted}
              />
              <StatTile
                label={t('dashboard.angie.connections.dropped')}
                value={angie.connections.dropped}
              />
            </div>
          )
        ) : (
          <p className="text-sm text-muted-foreground">
            {t('dashboard.angie.unreachableNote')}
          </p>
        )}
      </CardContent>
    </Card>
  )
}

function StatTile({ label, value }: { label: string; value: number }) {
  const { i18n } = useTranslation()
  return (
    <div className="rounded-lg border p-3">
      <div className="text-lg font-semibold tabular-nums">
        {formatNumber(value, i18n.language)}
      </div>
      <div className="text-xs text-muted-foreground">{label}</div>
    </div>
  )
}

// --- certificates ----------------------------------------------------------

function CertificatesSection({ certificates }: { certificates: DashboardCert[] }) {
  const { t } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('dashboard.certs.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        {certificates.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t('dashboard.certs.empty')}{' '}
            <Link
              to="/certificates"
              className="font-medium text-primary underline-offset-4 hover:underline"
            >
              {t('dashboard.certs.manage')}
            </Link>
          </p>
        ) : (
          <ul className="divide-y">
            {certificates.map((cert) => (
              <CertItem key={cert.id} cert={cert} />
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}

function CertItem({ cert }: { cert: DashboardCert }) {
  const { t } = useTranslation()
  // Matches the certificates page: "issued" is exactly certificate === "valid".
  const issued = cert.status?.certificate === 'valid'

  return (
    <li className="flex flex-wrap items-start justify-between gap-3 py-3 first:pt-0 last:pb-0">
      <div className="space-y-1">
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs">{cert.name}</span>
          {cert.staging && (
            <Badge className="bg-amber-600/15 text-amber-700 dark:bg-amber-400/15 dark:text-amber-400">
              {t('certificates.environment.staging')}
            </Badge>
          )}
        </div>
        <div className="flex flex-wrap gap-1">
          {cert.domains.map((domain) => (
            <Badge key={domain} variant="secondary">
              {domain}
            </Badge>
          ))}
        </div>
        {!issued && (
          <p className="text-xs text-muted-foreground">{t('dashboard.certs.autoHttps')}</p>
        )}
      </div>
      <StatusPill status={cert.status} />
    </li>
  )
}

// --- hosts -----------------------------------------------------------------

function HostsSection({ hosts }: { hosts: DashboardHost[] }) {
  const { t } = useTranslation()

  if (hosts.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('dashboard.hosts.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            {t('dashboard.hosts.empty')}{' '}
            <Link
              to="/hosts"
              className="font-medium text-primary underline-offset-4 hover:underline"
            >
              {t('dashboard.hosts.manage')}
            </Link>
          </p>
        </CardContent>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('dashboard.hosts.title')}</CardTitle>
      </CardHeader>
      <CardContent className="px-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="pl-4">{t('dashboard.hosts.table.domains')}</TableHead>
              <TableHead>{t('dashboard.hosts.table.target')}</TableHead>
              <TableHead>{t('dashboard.hosts.table.https')}</TableHead>
              <TableHead>{t('dashboard.hosts.table.requests')}</TableHead>
              <TableHead>{t('dashboard.hosts.table.responses')}</TableHead>
              <TableHead>{t('dashboard.hosts.table.upstream')}</TableHead>
              <TableHead className="pr-4">{t('dashboard.hosts.table.status')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {hosts.map((host) => (
              <HostRow key={host.id} host={host} />
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  )
}

function HostRow({ host }: { host: DashboardHost }) {
  const { t, i18n } = useTranslation()
  const responses = host.zone ? bucketResponses(host.zone.responses) : []

  return (
    <TableRow>
      <TableCell className="pl-4">
        <div className="flex flex-wrap gap-1">
          {host.domains.map((domain) => (
            <Badge key={domain} variant="secondary">
              {domain}
            </Badge>
          ))}
        </div>
      </TableCell>
      <TableCell>
        <span className="font-mono text-xs">{host.forward}</span>
      </TableCell>
      <TableCell>
        {host.https_active ? (
          <Badge className={EMERALD_BADGE}>{t('dashboard.hosts.https.on')}</Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('dashboard.hosts.https.off')}
          </Badge>
        )}
      </TableCell>
      <TableCell className="tabular-nums">
        {host.zone ? (
          formatNumber(host.zone.requests.total, i18n.language)
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell>
        {responses.length > 0 ? (
          <span className="font-mono text-xs whitespace-nowrap">
            {responses
              .map((entry) => `${entry.bucket}: ${formatNumber(entry.count, i18n.language)}`)
              .join('  ')}
          </span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell>
        <UpstreamCell upstream={host.upstream} />
      </TableCell>
      <TableCell className="pr-4">
        {host.enabled ? (
          <Badge className={EMERALD_BADGE}>{t('hosts.status.enabled')}</Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('hosts.status.disabled')}
          </Badge>
        )}
      </TableCell>
    </TableRow>
  )
}

function UpstreamCell({ upstream }: { upstream: DashboardUpstream | null }) {
  const { t } = useTranslation()

  if (upstream === null) {
    return <span className="text-muted-foreground">—</span>
  }

  const unhealthy = upstream.peers_down > 0 || upstream.fails > 0
  return (
    <span
      className={`text-sm whitespace-nowrap tabular-nums ${
        unhealthy ? 'font-medium text-red-700 dark:text-red-400' : 'text-muted-foreground'
      }`}
    >
      {t('dashboard.hosts.upstream.summary', {
        up: upstream.peers_up,
        down: upstream.peers_down,
      })}
    </span>
  )
}

interface ResponseBucket {
  bucket: string
  count: number
}

/**
 * Buckets Angie's server-zone `responses` map by HTTP status class (first
 * digit): "200" and "204" both fold into "2xx". Keys already in class form
 * ("2xx") are accepted; non-status keys ("total", "processing") are skipped.
 * Returns only non-empty classes, ordered 1xx → 5xx.
 */
function bucketResponses(responses: Record<string, number>): ResponseBucket[] {
  const totals = new Map<string, number>()
  for (const [key, value] of Object.entries(responses)) {
    if (typeof value !== 'number' || value <= 0) {
      continue
    }
    const match = /^([1-5])(?:\d{2}|xx)$/i.exec(key)
    if (match === null) {
      continue
    }
    const bucket = `${match[1]}xx`
    totals.set(bucket, (totals.get(bucket) ?? 0) + value)
  }
  return [...totals.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([bucket, count]) => ({ bucket, count }))
}

function formatNumber(value: number, language: string): string {
  return value.toLocaleString(language)
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span>{children}</span>
    </div>
  )
}

// --- config validation (kept from M0) --------------------------------------

function ConfigtestSection() {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()

  const reportQuery = useQuery({
    queryKey: CONFIGTEST_QUERY_KEY,
    queryFn: async (): Promise<ConfigtestReport | null> => {
      try {
        return await api.getLastConfigtest()
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) {
          // No configtest has ever been run.
          return null
        }
        throw error
      }
    },
    retry: false,
  })

  const runMutation = useMutation({
    mutationFn: () => api.runConfigtest(),
    onSuccess: (report) => {
      queryClient.setQueryData(CONFIGTEST_QUERY_KEY, report)
    },
  })

  const report = reportQuery.data ?? null

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('configtest.title')}</CardTitle>
        <CardDescription>{t('configtest.description')}</CardDescription>
        <CardAction>
          <Button onClick={() => runMutation.mutate()} disabled={runMutation.isPending}>
            {runMutation.isPending ? (
              <Loader2 className="animate-spin" aria-hidden="true" />
            ) : (
              <PlayCircle aria-hidden="true" />
            )}
            {runMutation.isPending ? t('configtest.running') : t('configtest.run')}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-3">
        {runMutation.isError && (
          <p role="alert" className="text-sm text-destructive">
            {runMutation.error instanceof ApiError
              ? runMutation.error.message
              : t('common.error')}
          </p>
        )}
        {reportQuery.isPending ? (
          <p className="text-sm text-muted-foreground">{t('common.loading')}</p>
        ) : report === null ? (
          <p className="text-sm text-muted-foreground">{t('configtest.never')}</p>
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
              {report.ok ? (
                <Badge className={EMERALD_BADGE}>{t('configtest.passed')}</Badge>
              ) : (
                <Badge variant="destructive">{t('configtest.failed')}</Badge>
              )}
              <span className="text-muted-foreground">
                {t('configtest.lastRun')}:{' '}
                {new Intl.DateTimeFormat(i18n.language, {
                  dateStyle: 'medium',
                  timeStyle: 'medium',
                }).format(new Date(report.timestamp * 1000))}
              </span>
              <span className="text-muted-foreground">
                {t('configtest.exitCode')}:{' '}
                <span className="font-mono">{report.exit_code ?? '—'}</span>
              </span>
              <span className="text-muted-foreground">
                {t('configtest.ranVia')}: {t(`configtest.${report.ran_via}`)}
              </span>
            </div>
            {/* Rendered as a React text node: always escaped, never interpreted as HTML. */}
            <pre className="max-h-80 overflow-auto rounded-lg border bg-muted/50 p-3 font-mono text-xs whitespace-pre-wrap">
              {report.output}
            </pre>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

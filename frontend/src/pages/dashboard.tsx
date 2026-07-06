import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, PlayCircle } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

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
import { api, ApiError, type ConfigtestReport } from '@/lib/api'

const CONFIGTEST_QUERY_KEY = ['system', 'configtest'] as const

export function DashboardPage() {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">{t('dashboard.title')}</h1>
      <SystemStatusSection />
      <ConfigtestSection />
    </div>
  )
}

function SystemStatusSection() {
  const { t } = useTranslation()
  const statusQuery = useQuery({
    queryKey: ['system', 'status'],
    queryFn: () => api.getSystemStatus(),
  })

  if (statusQuery.isPending) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" aria-hidden="true" />
        {t('common.loading')}
      </div>
    )
  }

  if (statusQuery.isError) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-3">
          <p className="text-sm text-destructive" role="alert">
            {t('dashboard.loadFailed')}
          </p>
          <Button variant="outline" size="sm" onClick={() => void statusQuery.refetch()}>
            {t('common.retry')}
          </Button>
        </CardContent>
      </Card>
    )
  }

  const status = statusQuery.data

  return (
    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      <Card>
        <CardHeader>
          <CardTitle>{t('dashboard.panel.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <StatusRow label={t('dashboard.panel.version')}>
            <span className="font-mono text-sm">{status.panel.version}</span>
          </StatusRow>
          <StatusRow label={t('dashboard.panel.dataDir')}>
            <BoolBadge
              value={status.panel.data_dir_writable}
              yesLabel={t('dashboard.panel.writable')}
              noLabel={t('dashboard.panel.readOnly')}
            />
          </StatusRow>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('dashboard.angie.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <StatusRow label={t('dashboard.angie.status')}>
            <BoolBadge
              value={status.angie.installed}
              yesLabel={t('dashboard.angie.installed')}
              noLabel={t('dashboard.angie.notInstalled')}
            />
          </StatusRow>
          <StatusRow label={t('dashboard.angie.version')}>
            <span className="font-mono text-sm">{status.angie.version ?? '—'}</span>
          </StatusRow>
          <StatusRow label={t('dashboard.angie.acme')}>
            <BoolBadge
              value={status.angie.acme_module}
              yesLabel={t('common.yes')}
              noLabel={t('common.no')}
            />
          </StatusRow>
          <StatusRow label={t('dashboard.angie.unit')}>
            <BoolBadge
              value={status.angie.unit_active}
              yesLabel={t('dashboard.angie.active')}
              noLabel={t('dashboard.angie.inactive')}
            />
          </StatusRow>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('dashboard.dbus.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <StatusRow label={t('dashboard.dbus.availability')}>
            <BoolBadge
              value={status.dbus.available}
              yesLabel={t('dashboard.dbus.available')}
              noLabel={t('dashboard.dbus.unavailable')}
            />
          </StatusRow>
          <StatusRow label={t('dashboard.dbus.polkit')}>
            <BoolBadge
              value={status.dbus.polkit_ok}
              yesLabel={t('common.ok')}
              noLabel={t('common.failed')}
            />
          </StatusRow>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('dashboard.statusApi.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <StatusRow label={t('dashboard.statusApi.availability')}>
            <BoolBadge
              value={status.status_api.reachable}
              yesLabel={t('dashboard.statusApi.reachable')}
              noLabel={t('dashboard.statusApi.unreachable')}
            />
          </StatusRow>
          <StatusRow label={t('dashboard.statusApi.generation')}>
            <span className="font-mono text-sm">{status.status_api.generation ?? '—'}</span>
          </StatusRow>
        </CardContent>
      </Card>
    </div>
  )
}

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
                <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
                  {t('configtest.passed')}
                </Badge>
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

function StatusRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span>{children}</span>
    </div>
  )
}

function BoolBadge({
  value,
  yesLabel,
  noLabel,
}: {
  value: boolean | null
  yesLabel: string
  noLabel: string
}) {
  const { t } = useTranslation()

  if (value === null) {
    return <Badge variant="outline">{t('common.unknown')}</Badge>
  }
  if (value) {
    return (
      <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
        {yesLabel}
      </Badge>
    )
  }
  return <Badge variant="destructive">{noLabel}</Badge>
}

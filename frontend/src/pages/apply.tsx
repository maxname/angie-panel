import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  AlertTriangle,
  CheckCircle2,
  FileWarning,
  Loader2,
  Rocket,
} from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
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
  type ApplyReport,
  type DiffReport,
  type FileDiff,
  type FileStatus,
} from '@/lib/api'
import { cn } from '@/lib/utils'

export function ApplyPage() {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [report, setReport] = useState<ApplyReport | null>(null)
  const [applyError, setApplyError] = useState<string | null>(null)

  const previewQuery = useQuery({
    queryKey: ['apply', 'preview'],
    queryFn: () => api.getApplyPreview(),
  })

  const historyQuery = useQuery({
    queryKey: ['apply', 'history'],
    queryFn: () => api.getApplyHistory(),
  })

  const applyMutation = useMutation({
    mutationFn: () => api.apply(),
    onMutate: () => {
      setReport(null)
      setApplyError(null)
    },
    onSuccess: (result) => {
      setReport(result)
    },
    onError: (error) => {
      setApplyError(error instanceof ApiError ? error.message : t('common.error'))
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ['apply', 'preview'] })
      void queryClient.invalidateQueries({ queryKey: ['apply', 'history'] })
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">{t('apply.title')}</h1>
        <Button
          onClick={() => applyMutation.mutate()}
          disabled={applyMutation.isPending}
        >
          {applyMutation.isPending ? (
            <Loader2 className="animate-spin" aria-hidden="true" />
          ) : (
            <Rocket aria-hidden="true" />
          )}
          {applyMutation.isPending ? t('apply.applying') : t('apply.apply')}
        </Button>
      </div>

      {applyError !== null && (
        <Alert variant="destructive">
          <AlertTriangle aria-hidden="true" />
          <AlertTitle>{t('apply.failed')}</AlertTitle>
          <AlertDescription>{applyError}</AlertDescription>
        </Alert>
      )}

      {report !== null && <ApplyResultCard report={report} />}

      <Card>
        <CardHeader>
          <CardTitle>{t('apply.preview.title')}</CardTitle>
          <CardDescription>{t('apply.preview.description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {previewQuery.isPending ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" aria-hidden="true" />
              {t('common.loading')}
            </div>
          ) : previewQuery.isError ? (
            <div className="flex flex-col items-start gap-3">
              <p className="text-sm text-destructive" role="alert">
                {t('apply.preview.loadFailed')}
              </p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void previewQuery.refetch()}
              >
                {t('common.retry')}
              </Button>
            </div>
          ) : (
            <PreviewBody diff={previewQuery.data.diff} />
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('apply.history.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          {historyQuery.isPending ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" aria-hidden="true" />
              {t('common.loading')}
            </div>
          ) : historyQuery.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {t('apply.history.loadFailed')}
            </p>
          ) : historyQuery.data.history.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t('apply.history.empty')}</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('apply.history.time')}</TableHead>
                  <TableHead>{t('apply.history.result')}</TableHead>
                  <TableHead>{t('apply.history.summary')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {historyQuery.data.history.map((entry) => (
                  <TableRow key={entry.id}>
                    <TableCell className="whitespace-nowrap text-muted-foreground">
                      {new Intl.DateTimeFormat(i18n.language, {
                        dateStyle: 'medium',
                        timeStyle: 'short',
                      }).format(new Date(entry.timestamp * 1000))}
                    </TableCell>
                    <TableCell>
                      <ResultBadge result={entry.result} />
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {entry.report.summary}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function PreviewBody({ diff }: { diff: DiffReport }) {
  const { t } = useTranslation()
  const changed = diff.files.filter((file) => file.status !== 'unchanged')

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap gap-2 text-sm">
        <CountPill label={t('apply.counts.added')} value={diff.added} tone="added" />
        <CountPill
          label={t('apply.counts.modified')}
          value={diff.modified}
          tone="modified"
        />
        <CountPill
          label={t('apply.counts.removed')}
          value={diff.removed}
          tone="removed"
        />
        <CountPill
          label={t('apply.counts.unchanged')}
          value={diff.unchanged}
          tone="unchanged"
        />
      </div>

      {diff.has_drift && (
        <Alert variant="warning">
          <AlertTriangle aria-hidden="true" />
          <AlertTitle>{t('apply.drift.title')}</AlertTitle>
          <AlertDescription>{t('apply.drift.body')}</AlertDescription>
        </Alert>
      )}

      {!diff.has_drift && changed.length === 0 && (
        <p className="text-sm text-muted-foreground">{t('apply.preview.noChanges')}</p>
      )}

      {changed.length > 0 && (
        <div className="space-y-4">
          {changed.map((file) => (
            <FileDiffBlock key={file.name} file={file} />
          ))}
        </div>
      )}

      {diff.foreign.length > 0 && (
        <Alert variant="info">
          <FileWarning aria-hidden="true" />
          <AlertTitle>{t('apply.foreign.title')}</AlertTitle>
          <AlertDescription>
            <p>{t('apply.foreign.body')}</p>
            <ul className="mt-1 list-inside list-disc font-mono text-xs">
              {diff.foreign.map((item) => (
                <li key={item.name}>{item.name}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}
    </div>
  )
}

function FileDiffBlock({ file }: { file: FileDiff }) {
  const hasDiff = file.unified.trim() !== ''

  return (
    <div className="rounded-lg border">
      {/* The muted header bar + bottom divider only make sense when a unified
          diff follows below. Added files carry no diff, so keep the row clean. */}
      <div
        className={cn(
          'flex flex-wrap items-center gap-2 px-3 py-2',
          hasDiff && 'border-b bg-muted/40',
        )}
      >
        <StatusBadge status={file.status} />
        <span className="font-mono text-xs">{file.name}</span>
        {file.drift && <DriftTag />}
      </div>
      {hasDiff && <DiffView unified={file.unified} />}
    </div>
  )
}

/**
 * Renders a unified diff safely: the raw text is split into lines and each line
 * becomes a React text node (never HTML), coloured by its leading marker.
 */
function DiffView({ unified }: { unified: string }) {
  const lines = unified.replace(/\n$/, '').split('\n')

  return (
    <pre className="max-h-96 overflow-auto p-0 font-mono text-xs leading-relaxed">
      <code className="block">
        {lines.map((line, index) => (
          <span key={index} className={diffLineClass(line)}>
            {line === '' ? ' ' : line}
            {'\n'}
          </span>
        ))}
      </code>
    </pre>
  )
}

function diffLineClass(line: string): string {
  if (line.startsWith('+++') || line.startsWith('---')) {
    return 'block px-3 font-semibold text-muted-foreground'
  }
  if (line.startsWith('@@')) {
    return 'block bg-sky-500/10 px-3 text-sky-700 dark:text-sky-300'
  }
  if (line.startsWith('+')) {
    return 'block bg-success/10 px-3 text-success'
  }
  if (line.startsWith('-')) {
    return 'block bg-destructive/10 px-3 text-destructive'
  }
  return 'block px-3 text-muted-foreground'
}

function ApplyResultCard({ report }: { report: ApplyReport }) {
  const { t } = useTranslation()

  if (report.result === 'ok') {
    return (
      <Alert variant="success">
        <CheckCircle2 aria-hidden="true" />
        <AlertTitle>{t('apply.result.okTitle')}</AlertTitle>
        <AlertDescription>{report.summary}</AlertDescription>
      </Alert>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ResultBadge result={report.result} />
          {report.summary}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {report.result === 'lint_failed' && report.lint_violations.length > 0 && (
          <Section title={t('apply.result.lintViolations')}>
            <ul className="space-y-1 text-sm">
              {report.lint_violations.map((violation, index) => (
                <li key={index} className="font-mono text-xs">
                  {violation.file}
                  {violation.line !== null ? `:${violation.line}` : ''} —{' '}
                  {violation.message}
                </li>
              ))}
            </ul>
          </Section>
        )}

        {report.result === 'validation_failed' && (
          <>
            {report.file_errors.length > 0 && (
              <Section title={t('apply.result.fileErrors')}>
                <ul className="space-y-1 text-sm">
                  {report.file_errors.map((fileError, index) => (
                    <li key={index} className="font-mono text-xs">
                      {fileError.file ?? t('apply.result.unknownFile')}
                      {fileError.line !== null ? `:${fileError.line}` : ''} —{' '}
                      {fileError.message}
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            {report.stderr.trim() !== '' && (
              <Section title={t('apply.result.stderr')}>
                <LogBlock text={report.stderr} />
              </Section>
            )}
          </>
        )}

        {report.result === 'reload_failed' && (
          <>
            {report.error_log_tail.trim() !== '' && (
              <Section title={t('apply.result.errorLog')}>
                <LogBlock text={report.error_log_tail} />
              </Section>
            )}
            {report.rollback !== undefined && (
              <Section title={t('apply.result.rollback')}>
                <p className="text-sm">
                  {report.rollback.attempted
                    ? report.rollback.ok
                      ? t('apply.result.rollbackOk')
                      : t('apply.result.rollbackFailed')
                    : t('apply.result.rollbackNotAttempted')}
                </p>
                {report.rollback.detail.trim() !== '' && (
                  <LogBlock text={report.rollback.detail} />
                )}
              </Section>
            )}
          </>
        )}

        {report.result === 'error' && report.stderr.trim() !== '' && (
          <Section title={t('apply.result.stderr')}>
            <LogBlock text={report.stderr} />
          </Section>
        )}
      </CardContent>
    </Card>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium">{title}</p>
      {children}
    </div>
  )
}

function LogBlock({ text }: { text: string }) {
  return (
    <pre className="max-h-64 overflow-auto rounded-lg border bg-muted/50 p-3 font-mono text-xs whitespace-pre-wrap">
      {text}
    </pre>
  )
}

function CountPill({
  label,
  value,
  tone,
}: {
  label: string
  value: number
  tone: 'added' | 'modified' | 'removed' | 'unchanged'
}) {
  const toneClass = {
    added: 'text-success',
    modified: 'text-warning',
    removed: 'text-destructive',
    unchanged: 'text-muted-foreground',
  }[tone]

  return (
    <span className="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5">
      <span className={`text-base font-semibold tabular-nums ${toneClass}`}>{value}</span>
      <span className="text-muted-foreground">{label}</span>
    </span>
  )
}

function StatusBadge({ status }: { status: FileStatus }) {
  const { t } = useTranslation()
  const map: Record<FileStatus, string> = {
    added: 'bg-success/10 text-success',
    modified: 'bg-warning/10 text-warning',
    removed: 'bg-destructive/10 text-destructive',
    unchanged: 'text-muted-foreground',
  }
  return <Badge className={map[status]}>{t(`apply.status.${status}`)}</Badge>
}

function DriftTag() {
  const { t } = useTranslation()
  return (
    <Badge variant="destructive" className="gap-1">
      <AlertTriangle aria-hidden="true" />
      {t('apply.drift.tag')}
    </Badge>
  )
}

function ResultBadge({ result }: { result: string }) {
  const { t } = useTranslation()
  const label =
    result === 'ok' ||
    result === 'lint_failed' ||
    result === 'validation_failed' ||
    result === 'reload_failed' ||
    result === 'error'
      ? t(`apply.result.${result}`)
      : result

  if (result === 'ok') {
    return (
      <Badge variant="success">
        {label}
      </Badge>
    )
  }
  return <Badge variant="destructive">{label}</Badge>
}

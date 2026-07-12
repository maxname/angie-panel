import { useQuery } from '@tanstack/react-query'
import { Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, type AuditEntry } from '@/lib/api'

/** Colour + label for the HTTP verb (mutations only reach the audit log). */
function verb(method: string): { label: string; className: string } {
  switch (method) {
    case 'POST':
      return {
        label: 'POST',
        className:
          'bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400',
      }
    case 'PUT':
      return {
        label: 'PUT',
        className:
          'bg-sky-600/15 text-sky-700 dark:bg-sky-400/15 dark:text-sky-400',
      }
    case 'DELETE':
      return {
        label: 'DELETE',
        className:
          'bg-rose-600/15 text-rose-700 dark:bg-rose-400/15 dark:text-rose-400',
      }
    default:
      return { label: method, className: 'bg-muted text-muted-foreground' }
  }
}

/** A readable i18n key for what an endpoint acts on, from its path. */
function targetKey(path: string): string {
  const parts = path.replace(/^\/api\//, '').split('/')
  if (parts[0] === 'auth') return `audit.target.auth_${parts[1] ?? ''}`
  if (parts[0] === 'users' && parts[1] === 'me') return 'audit.target.own_password'
  return `audit.target.${(parts[0] ?? '').replace(/-/g, '_')}`
}

function statusClass(status: number): string {
  if (status >= 500) {
    return 'bg-rose-600/15 text-rose-700 dark:bg-rose-400/15 dark:text-rose-400'
  }
  if (status >= 400) {
    return 'bg-amber-600/15 text-amber-700 dark:bg-amber-400/15 dark:text-amber-400'
  }
  return 'bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400'
}

export function AuditLogPage() {
  const { t, i18n } = useTranslation()
  const auditQuery = useQuery({ queryKey: ['audit'], queryFn: () => api.listAudit() })

  const fmt = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })

  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('audit.title')}
        </h1>
        <p className="text-sm text-muted-foreground">{t('audit.subtitle')}</p>
      </div>

      {auditQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : auditQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('audit.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void auditQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : auditQuery.data.entries.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('audit.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('audit.table.time')}</TableHead>
                    <TableHead>{t('audit.table.user')}</TableHead>
                    <TableHead>{t('audit.table.action')}</TableHead>
                    <TableHead className="text-right">
                      {t('audit.table.status')}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {auditQuery.data.entries.map((entry) => (
                    <AuditRow key={entry.id} entry={entry} fmt={fmt} />
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}

function AuditRow({
  entry,
  fmt,
}: {
  entry: AuditEntry
  fmt: Intl.DateTimeFormat
}) {
  const { t } = useTranslation()
  const v = verb(entry.method)
  const parts = entry.path.replace(/^\/api\//, '').split('/')
  const target = t(targetKey(entry.path), { defaultValue: parts[0] ?? entry.path })

  return (
    <TableRow>
      <TableCell className="whitespace-nowrap text-sm text-muted-foreground">
        {fmt.format(new Date(entry.created_at * 1000))}
      </TableCell>
      <TableCell className="text-sm">
        {entry.user_email ?? (
          <span className="text-muted-foreground">{t('audit.anonymous')}</span>
        )}
      </TableCell>
      <TableCell>
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <Badge className={v.className}>{v.label}</Badge>
            <span className="text-sm">{target}</span>
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {entry.path}
          </span>
        </div>
      </TableCell>
      <TableCell className="text-right">
        <Badge className={statusClass(entry.status)}>{entry.status}</Badge>
      </TableCell>
    </TableRow>
  )
}

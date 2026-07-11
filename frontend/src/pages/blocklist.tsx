import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, ApiError, type Ban } from '@/lib/api'
import { toast } from '@/lib/toast'

export function BlocklistPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [address, setAddress] = useState('')
  const [reason, setReason] = useState('')
  const [error, setError] = useState<string | null>(null)

  const bansQuery = useQuery({ queryKey: ['bans'], queryFn: () => api.listBans() })

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['bans'] })
  const unapplied = () =>
    toast({
      title: t('blocklist.unappliedTitle'),
      description: t('blocklist.unappliedBody'),
    })

  const addMutation = useMutation({
    mutationFn: () =>
      api.createBan({ address, reason: reason.trim() === '' ? null : reason }),
    onSuccess: () => {
      void invalidate()
      setAddress('')
      setReason('')
      setError(null)
      unapplied()
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : t('common.error')),
  })

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError(null)
    addMutation.mutate()
  }

  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('blocklist.title')}
        </h1>
        <p className="text-sm text-muted-foreground">{t('blocklist.subtitle')}</p>
      </div>

      <Card>
        <CardContent>
          <form className="flex flex-wrap items-end gap-3" onSubmit={submit} noValidate>
            <div className="space-y-2">
              <Label htmlFor="ban-address">{t('blocklist.address')}</Label>
              <Input
                id="ban-address"
                className="w-56 font-mono"
                placeholder="203.0.113.7"
                value={address}
                onChange={(e) => setAddress(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="ban-reason">{t('blocklist.reason')}</Label>
              <Input
                id="ban-reason"
                className="w-64"
                placeholder={t('blocklist.reasonPlaceholder')}
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </div>
            <Button type="submit" disabled={addMutation.isPending}>
              {addMutation.isPending ? (
                <Loader2 className="animate-spin" aria-hidden="true" />
              ) : (
                <Plus aria-hidden="true" />
              )}
              {t('blocklist.add')}
            </Button>
          </form>
          {error !== null && (
            <Alert variant="destructive" className="mt-3">
              <AlertTitle>{t('blocklist.addFailed')}</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <p className="mt-3 text-xs text-muted-foreground">{t('blocklist.hint')}</p>
        </CardContent>
      </Card>

      {bansQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : bansQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('blocklist.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void bansQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : bansQuery.data.bans.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('blocklist.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('blocklist.table.address')}</TableHead>
                  <TableHead>{t('blocklist.table.reason')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('blocklist.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {bansQuery.data.bans.map((ban) => (
                  <BanRow key={ban.id} ban={ban} onRemoved={unapplied} />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  )
}

function BanRow({ ban, onRemoved }: { ban: Ban; onRemoved: () => void }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteBan(ban.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['bans'] })
      onRemoved()
    },
    onError: (error) =>
      toast({
        variant: 'destructive',
        title: t('blocklist.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      }),
  })

  return (
    <TableRow>
      <TableCell className="font-mono text-sm">{ban.address}</TableCell>
      <TableCell className="text-sm text-muted-foreground">
        {ban.reason ?? '—'}
      </TableCell>
      <TableCell className="text-right">
        <Button
          variant="ghost"
          size="sm"
          className="text-destructive"
          disabled={deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          {deleteMutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('blocklist.unban')}
        </Button>
      </TableCell>
    </TableRow>
  )
}

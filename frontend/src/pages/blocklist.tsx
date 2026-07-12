import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, Plus, X } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, ApiError, type Ban, type GeoMode, type GeoPolicy } from '@/lib/api'
import { COUNTRIES, countryName } from '@/lib/countries'
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

      <CountryPolicyCard />

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

/** Global country allow/deny policy (GeoIP). One policy for the whole panel. */
function CountryPolicyCard() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const geoQuery = useQuery({ queryKey: ['geo'], queryFn: () => api.getGeo() })

  const [draft, setDraft] = useState<{ mode: GeoMode; countries: string[] }>({
    mode: 'off',
    countries: [],
  })
  const [seededFrom, setSeededFrom] = useState<GeoPolicy | undefined>(undefined)
  const [error, setError] = useState<string | null>(null)

  // Seed the editable draft from the loaded policy without an effect (the
  // React "adjust state during render" idiom; defensive against partial data).
  if (geoQuery.data && geoQuery.data !== seededFrom) {
    setSeededFrom(geoQuery.data)
    setDraft({
      mode: geoQuery.data.mode ?? 'off',
      countries: geoQuery.data.countries ?? [],
    })
  }
  const { mode, countries } = draft
  const setMode = (m: GeoMode) => setDraft((d) => ({ ...d, mode: m }))

  const saveMutation = useMutation({
    mutationFn: () => api.putGeo({ mode, countries }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['geo'] })
      setError(null)
      toast({
        title: t('blocklist.unappliedTitle'),
        description: t('blocklist.unappliedBody'),
      })
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : t('common.error')),
  })

  const addCountry = (code: string) => {
    if (code && !countries.includes(code)) {
      setDraft((d) => ({ ...d, countries: [...d.countries, code] }))
    }
  }
  const removeCountry = (code: string) =>
    setDraft((d) => ({ ...d, countries: d.countries.filter((c) => c !== code) }))

  const available = COUNTRIES.filter((c) => !countries.includes(c.code))

  return (
    <Card>
      <CardContent className="space-y-4">
        <div className="space-y-1">
          <h2 className="text-sm font-medium">{t('blocklist.geo.title')}</h2>
          <p className="text-xs text-muted-foreground">
            {t('blocklist.geo.description')}
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <div className="space-y-2">
            <Label htmlFor="geo-mode">{t('blocklist.geo.mode')}</Label>
            <Select value={mode} onValueChange={(v) => setMode(v as GeoMode)}>
              <SelectTrigger id="geo-mode" className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">{t('blocklist.geo.modeOff')}</SelectItem>
                <SelectItem value="deny">{t('blocklist.geo.modeDeny')}</SelectItem>
                <SelectItem value="allow">{t('blocklist.geo.modeAllow')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {mode !== 'off' && (
            <div className="space-y-2">
              <Label htmlFor="geo-add-country">
                {t('blocklist.geo.addCountry')}
              </Label>
              {/* Empty controlled value so the picker resets after each pick. */}
              <Select value="" onValueChange={addCountry}>
                <SelectTrigger id="geo-add-country" className="w-64">
                  <SelectValue placeholder={t('blocklist.geo.addCountry')} />
                </SelectTrigger>
                <SelectContent>
                  {available.map((c) => (
                    <SelectItem key={c.code} value={c.code}>
                      {c.name} ({c.code})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          <Button onClick={() => saveMutation.mutate()} disabled={saveMutation.isPending}>
            {saveMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('common.save')}
          </Button>
        </div>

        {mode !== 'off' && (
          <div className="flex flex-wrap gap-2">
            {countries.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {t('blocklist.geo.noCountries')}
              </p>
            ) : (
              countries.map((code) => (
                <Badge key={code} variant="secondary" className="gap-1">
                  {countryName(code)} ({code})
                  <button
                    type="button"
                    aria-label={t('blocklist.geo.remove', { country: code })}
                    onClick={() => removeCountry(code)}
                    className="ml-0.5 rounded-sm hover:text-destructive"
                  >
                    <X className="size-3" aria-hidden="true" />
                  </button>
                </Badge>
              ))
            )}
          </div>
        )}

        {error !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('blocklist.geo.saveFailed')}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        <p className="text-xs text-muted-foreground">{t('blocklist.geo.hint')}</p>
      </CardContent>
    </Card>
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

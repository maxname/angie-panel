import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Copy, Loader2, Plus, Terminal } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { api, ApiError, type ApiToken } from '@/lib/api'
import { toast } from '@/lib/toast'

export function TokensPage() {
  const { t, i18n } = useTranslation()
  const [createOpen, setCreateOpen] = useState(false)
  // Held in memory only, and only until the dialog closes: the API returns the
  // secret exactly once and there is no way to look it up again.
  const [issued, setIssued] = useState<{ name: string; secret: string } | null>(null)

  const tokensQuery = useQuery({ queryKey: ['tokens'], queryFn: () => api.listTokens() })
  const fmt = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t('tokens.title')}
          </h1>
          <Button className="shrink-0" onClick={() => setCreateOpen(true)}>
            <Plus aria-hidden="true" />
            {t('tokens.add')}
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">{t('tokens.subtitle')}</p>
      </div>

      <Card>
        <CardContent className="space-y-2">
          <h2 className="flex items-center gap-2 text-sm font-medium">
            <Terminal className="size-4" aria-hidden="true" />
            {t('tokens.cli.title')}
          </h2>
          <p className="text-sm text-muted-foreground">{t('tokens.cli.body')}</p>
          <pre className="overflow-x-auto rounded-md bg-muted p-3 text-xs">
            <code>{'export ANGIE_PANEL_TOKEN=ap_…\napctl --url https://panel.example.com status'}</code>
          </pre>
        </CardContent>
      </Card>

      {tokensQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : tokensQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('tokens.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void tokensQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : tokensQuery.data.tokens.length === 0 ? (
        <Card>
          <CardContent>
            <p className="text-sm text-muted-foreground">{t('tokens.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('tokens.table.name')}</TableHead>
                  <TableHead>{t('tokens.table.prefix')}</TableHead>
                  <TableHead>{t('tokens.table.owner')}</TableHead>
                  <TableHead>{t('tokens.table.lastUsed')}</TableHead>
                  <TableHead>{t('tokens.table.expires')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('tokens.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tokensQuery.data.tokens.map((token) => (
                  <TokenRow
                    key={token.id}
                    token={token}
                    fmt={fmt}
                    // When the list was fetched, rather than Date.now() during
                    // render: a stable value, and the honest basis for
                    // "expired" — it is what the server told us and when.
                    asOf={tokensQuery.dataUpdatedAt}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <CreateTokenDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onIssued={setIssued}
      />
      <IssuedTokenDialog issued={issued} onClose={() => setIssued(null)} />
    </div>
  )
}

function TokenRow({
  token,
  fmt,
  asOf,
}: {
  token: ApiToken
  fmt: Intl.DateTimeFormat
  asOf: number
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [confirmDelete, setConfirmDelete] = useState(false)

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteToken(token.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tokens'] })
      setConfirmDelete(false)
    },
    onError: (error: unknown) =>
      toast({
        variant: 'destructive',
        title: t('tokens.actionFailed'),
        description: error instanceof ApiError ? error.message : t('common.error'),
      }),
  })

  const expired = token.expires_at !== null && token.expires_at * 1000 < asOf

  return (
    <TableRow>
      <TableCell className="font-medium">
        {token.name}
        {token.is_local && (
          <Badge variant="secondary" className="ml-2">
            {t('tokens.localBadge')}
          </Badge>
        )}
        {expired && (
          <Badge variant="destructive" className="ml-2">
            {t('tokens.expiredBadge')}
          </Badge>
        )}
      </TableCell>
      <TableCell className="font-mono text-xs text-muted-foreground">
        {token.prefix}
      </TableCell>
      <TableCell className="text-sm">
        {token.owner ?? t('tokens.noOwner')}
      </TableCell>
      <TableCell className="text-sm text-muted-foreground">
        {token.last_used_at === null
          ? t('tokens.neverUsed')
          : fmt.format(new Date(token.last_used_at * 1000))}
      </TableCell>
      <TableCell className="text-sm text-muted-foreground">
        {token.expires_at === null
          ? t('tokens.neverExpires')
          : fmt.format(new Date(token.expires_at * 1000))}
      </TableCell>
      <TableCell className="text-right">
        <Button
          variant="ghost"
          size="sm"
          // The local token is rotated on disk, not revoked here — the server
          // rejects the call, so don't offer it.
          disabled={token.is_local || deleteMutation.isPending}
          onClick={() => setConfirmDelete(true)}
        >
          {t('tokens.revoke')}
        </Button>

        <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
          <DialogContent className="max-w-sm">
            <DialogHeader>
              <DialogTitle>{t('tokens.revokeConfirm.title')}</DialogTitle>
              <DialogDescription>
                {t('tokens.revokeConfirm.body', { name: token.name })}
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => setConfirmDelete(false)}
                disabled={deleteMutation.isPending}
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="destructive"
                onClick={() => deleteMutation.mutate()}
                disabled={deleteMutation.isPending}
              >
                {deleteMutation.isPending && (
                  <Loader2 className="animate-spin" aria-hidden="true" />
                )}
                {t('tokens.revoke')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </TableCell>
    </TableRow>
  )
}

function CreateTokenDialog({
  open,
  onOpenChange,
  onIssued,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: { name: string; secret: string }) => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [expiry, setExpiry] = useState('')
  const [error, setError] = useState<string | null>(null)

  const reset = () => {
    setName('')
    setExpiry('')
    setError(null)
  }

  const mutation = useMutation({
    mutationFn: () => {
      const days = expiry.trim() === '' ? undefined : Number(expiry)
      return api.createToken({ name, ...(days === undefined ? {} : { expires_in_days: days }) })
    },
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: ['tokens'] })
      onOpenChange(false)
      reset()
      onIssued({ name: created.name, secret: created.secret })
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : t('common.error')),
  })

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError(null)
    mutation.mutate()
  }

  const handleOpenChange = (next: boolean) => {
    if (!next) reset()
    onOpenChange(next)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('tokens.create.title')}</DialogTitle>
          <DialogDescription>{t('tokens.create.description')}</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
          <div className="space-y-2">
            <Label htmlFor="new-token-name">{t('tokens.create.name')}</Label>
            <Input
              id="new-token-name"
              spellCheck={false}
              autoCapitalize="off"
              placeholder={t('tokens.create.namePlaceholder')}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-token-expiry">{t('tokens.create.expiry')}</Label>
            <Input
              id="new-token-expiry"
              type="number"
              min={1}
              max={3650}
              placeholder={t('tokens.create.expiryPlaceholder')}
              value={expiry}
              onChange={(e) => setExpiry(e.target.value)}
            />
            <p className="text-xs text-muted-foreground">
              {t('tokens.create.expiryHint')}
            </p>
          </div>
          {error !== null && (
            <Alert variant="destructive">
              <AlertTitle>{t('tokens.create.failed')}</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={mutation.isPending}
            >
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending && <Loader2 className="animate-spin" aria-hidden="true" />}
              {t('tokens.create.submit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** The one and only time the secret is visible. Closing this dialog loses it. */
function IssuedTokenDialog({
  issued,
  onClose,
}: {
  issued: { name: string; secret: string } | null
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)

  const copy = () => {
    if (issued === null) return
    // navigator.clipboard is unavailable over plain HTTP outside localhost,
    // which is exactly how many operators reach a LAN panel. Fall back to
    // selecting the text so copying by hand still works.
    void navigator.clipboard
      ?.writeText(issued.secret)
      .then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch(() =>
        toast({ variant: 'destructive', title: t('tokens.issued.copyFailed') }),
      )
  }

  return (
    <Dialog
      open={issued !== null}
      onOpenChange={(next) => {
        if (!next) {
          setCopied(false)
          onClose()
        }
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('tokens.issued.title', { name: issued?.name ?? '' })}</DialogTitle>
          <DialogDescription>{t('tokens.issued.description')}</DialogDescription>
        </DialogHeader>
        <Alert>
          <AlertTitle>{t('tokens.issued.onceTitle')}</AlertTitle>
          <AlertDescription>{t('tokens.issued.onceBody')}</AlertDescription>
        </Alert>
        <div className="flex items-center gap-2">
          <Input
            readOnly
            className="font-mono text-xs"
            value={issued?.secret ?? ''}
            aria-label={t('tokens.issued.secretLabel')}
            onFocus={(e) => e.currentTarget.select()}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            onClick={copy}
            aria-label={t('tokens.issued.copy')}
          >
            {copied ? (
              <Check aria-hidden="true" />
            ) : (
              <Copy aria-hidden="true" />
            )}
          </Button>
        </div>
        <DialogFooter>
          <Button type="button" onClick={onClose}>
            {t('tokens.issued.done')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2 } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

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
import { api, ApiError, type DnsProviderInfo } from '@/lib/api'
import { toast } from '@/lib/toast'

export function DnsProvidersPage() {
  const { t } = useTranslation()
  const [editing, setEditing] = useState<DnsProviderInfo | null>(null)

  const providersQuery = useQuery({
    queryKey: ['dns-providers'],
    queryFn: () => api.listDnsProviders(),
  })
  const providers = providersQuery.data?.providers ?? []
  const configuredCount = providers.filter((p) => p.configured).length

  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('dnsProviders.title')}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t('dnsProviders.subtitle')}
        </p>
      </div>

      {providersQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : providersQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('dnsProviders.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void providersQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <>
          <p className="text-sm text-muted-foreground">
            {t('dnsProviders.summary', { count: configuredCount })}
          </p>
          <Card className="p-0">
            <CardContent className="px-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('dnsProviders.table.provider')}</TableHead>
                    <TableHead>{t('dnsProviders.table.status')}</TableHead>
                    <TableHead className="w-0 text-right">
                      <span className="sr-only">
                        {t('dnsProviders.table.actions')}
                      </span>
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {providers.map((provider) => (
                    <TableRow key={provider.id}>
                      <TableCell className="font-medium">
                        {provider.label}
                      </TableCell>
                      <TableCell>
                        {provider.configured ? (
                          <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
                            {t('dnsProviders.status.configured')}
                          </Badge>
                        ) : (
                          <Badge
                            variant="outline"
                            className="text-muted-foreground"
                          >
                            {t('dnsProviders.status.notConfigured')}
                          </Badge>
                        )}
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setEditing(provider)}
                        >
                          {provider.configured
                            ? t('dnsProviders.edit')
                            : t('dnsProviders.configure')}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </>
      )}

      <ProviderEditorDialog
        provider={editing}
        onOpenChange={(open) => {
          if (!open) setEditing(null)
        }}
      />
    </div>
  )
}

interface ProviderEditorDialogProps {
  provider: DnsProviderInfo | null
  onOpenChange: (open: boolean) => void
}

function ProviderEditorDialog({
  provider,
  onOpenChange,
}: ProviderEditorDialogProps) {
  const { t } = useTranslation()
  return (
    <Dialog open={provider !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t('dnsProviders.editor.title', { provider: provider?.label ?? '' })}
          </DialogTitle>
          <DialogDescription>{t('dnsProviders.editor.hint')}</DialogDescription>
        </DialogHeader>
        {provider !== null && (
          <ProviderCredentialsForm
            key={provider.id}
            provider={provider}
            onDone={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function ProviderCredentialsForm({
  provider,
  onDone,
}: {
  provider: DnsProviderInfo
  onDone: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [values, setValues] = useState<Record<string, string>>({})

  const mutation = useMutation({
    mutationFn: (creds: Record<string, string>) =>
      api.setDnsProviderCredentials(provider.id, creds),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dns-providers'] })
      toast({ variant: 'success', title: t('dnsProviders.saved') })
      onDone()
    },
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const creds: Record<string, string> = {}
    for (const field of provider.fields) {
      creds[field.env] = (values[field.env] ?? '').trim()
    }
    mutation.mutate(creds)
  }

  const disconnect = () => {
    const creds: Record<string, string> = {}
    for (const field of provider.fields) creds[field.env] = ''
    mutation.mutate(creds)
  }

  const error =
    mutation.isError && mutation.error instanceof ApiError
      ? mutation.error.message
      : mutation.isError
        ? t('common.error')
        : null

  const canSave = provider.fields.every(
    (f) => (values[f.env] ?? '').trim() !== '',
  )

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {provider.fields.map((field) => (
        <div key={field.env} className="grid gap-2">
          <Label htmlFor={`dns-cred-${field.env}`}>{field.label}</Label>
          <Input
            id={`dns-cred-${field.env}`}
            type="password"
            autoComplete="new-password"
            placeholder={provider.configured ? '••••••••' : ''}
            value={values[field.env] ?? ''}
            onChange={(e) =>
              setValues((v) => ({ ...v, [field.env]: e.target.value }))
            }
          />
        </div>
      ))}
      <p className="text-xs text-muted-foreground">
        {t('dnsProviders.editor.secretNote')}
      </p>
      {error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <DialogFooter className="gap-2 sm:justify-between">
        {provider.configured ? (
          <Button
            type="button"
            variant="outline"
            disabled={mutation.isPending}
            onClick={disconnect}
          >
            {t('dnsProviders.disconnect')}
          </Button>
        ) : (
          <span />
        )}
        <div className="flex items-center gap-2">
          <Button type="button" variant="outline" onClick={onDone}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" disabled={mutation.isPending || !canSave}>
            {mutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('dnsProviders.editor.save')}
          </Button>
        </div>
      </DialogFooter>
    </form>
  )
}

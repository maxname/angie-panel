import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, MoreHorizontal, Plus } from 'lucide-react'
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
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
import {
  api,
  ApiError,
  type DnsCredentialProfile,
  type DnsProviderInfo,
} from '@/lib/api'
import { toast } from '@/lib/toast'

export function DnsProvidersPage() {
  const { t } = useTranslation()
  const [editing, setEditing] = useState<DnsCredentialProfile | null>(null)
  const [creating, setCreating] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<DnsCredentialProfile | null>(
    null,
  )

  const profilesQuery = useQuery({
    queryKey: ['dns-credentials'],
    queryFn: () => api.listDnsCredentials(),
  })
  const typesQuery = useQuery({
    queryKey: ['dns-providers'],
    queryFn: () => api.listDnsProviders(),
  })
  const profiles = profilesQuery.data?.credentials ?? []
  const types = typesQuery.data?.providers ?? []

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t('dnsProviders.title')}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t('dnsProviders.subtitle')}
          </p>
        </div>
        <Button
          className="shrink-0"
          onClick={() => setCreating(true)}
          disabled={types.length === 0}
        >
          <Plus aria-hidden="true" />
          {t('dnsProviders.add')}
        </Button>
      </div>

      {profilesQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : profilesQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('dnsProviders.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void profilesQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : profiles.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">
              {t('dnsProviders.empty')}
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('dnsProviders.table.name')}</TableHead>
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
                {profiles.map((profile) => (
                  <TableRow key={profile.id}>
                    <TableCell className="font-medium">{profile.name}</TableCell>
                    <TableCell className="text-muted-foreground">
                      {profile.provider_label}
                    </TableCell>
                    <TableCell>
                      {profile.configured ? (
                        <Badge className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400">
                          {t('dnsProviders.status.configured')}
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="text-muted-foreground">
                          {t('dnsProviders.status.notConfigured')}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={t('dnsProviders.table.actions')}
                          >
                            <MoreHorizontal aria-hidden="true" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem onSelect={() => setEditing(profile)}>
                            {t('dnsProviders.edit')}
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            variant="destructive"
                            onSelect={() => setDeleteTarget(profile)}
                          >
                            {t('dnsProviders.delete')}
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <ProfileDialog
        open={creating || editing !== null}
        profile={editing}
        types={types}
        onOpenChange={(open) => {
          if (!open) {
            setCreating(false)
            setEditing(null)
          }
        }}
      />
      <DeleteProfileDialog
        profile={deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      />
    </div>
  )
}

interface ProfileDialogProps {
  open: boolean
  /** The profile being edited, or null when creating a new one. */
  profile: DnsCredentialProfile | null
  types: DnsProviderInfo[]
  onOpenChange: (open: boolean) => void
}

function ProfileDialog({
  open,
  profile,
  types,
  onOpenChange,
}: ProfileDialogProps) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {profile === null
              ? t('dnsProviders.editor.createTitle')
              : t('dnsProviders.editor.editTitle', { name: profile.name })}
          </DialogTitle>
          <DialogDescription>{t('dnsProviders.editor.hint')}</DialogDescription>
        </DialogHeader>
        {open && (
          <ProfileForm
            key={profile?.id ?? 'new'}
            profile={profile}
            types={types}
            onDone={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function ProfileForm({
  profile,
  types,
  onDone,
}: {
  profile: DnsCredentialProfile | null
  types: DnsProviderInfo[]
  onDone: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const isEdit = profile !== null

  const [providerId, setProviderId] = useState(
    profile?.provider ?? types[0]?.id ?? '',
  )
  const [name, setName] = useState(profile?.name ?? '')
  const [values, setValues] = useState<Record<string, string>>({})

  const type = types.find((tp) => tp.id === providerId)

  const mutation = useMutation({
    mutationFn: () => {
      const credentials: Record<string, string> = {}
      for (const field of type?.fields ?? []) {
        const v = (values[field.env] ?? '').trim()
        // On create send all; on edit only send fields the user actually typed.
        if (!isEdit || v !== '') credentials[field.env] = v
      }
      if (isEdit) {
        return api.updateDnsCredential(profile.id, { name: name.trim(), credentials })
      }
      return api.createDnsCredential({
        provider: providerId,
        name: name.trim(),
        credentials,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dns-credentials'] })
      toast({ variant: 'success', title: t('dnsProviders.saved') })
      onDone()
    },
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    mutation.mutate()
  }

  const error =
    mutation.isError && mutation.error instanceof ApiError
      ? mutation.error.message
      : mutation.isError
        ? t('common.error')
        : null

  // Create requires every field; edit lets you change the name alone.
  const allCredsFilled = (type?.fields ?? []).every(
    (f) => (values[f.env] ?? '').trim() !== '',
  )
  const canSave =
    name.trim() !== '' && (isEdit || (providerId !== '' && allCredsFilled))

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div className="grid gap-2">
        <Label htmlFor="profile-provider">{t('dnsProviders.editor.provider')}</Label>
        {isEdit ? (
          <Input
            id="profile-provider"
            value={profile.provider_label}
            disabled
            readOnly
          />
        ) : (
          <Select value={providerId} onValueChange={setProviderId}>
            <SelectTrigger id="profile-provider">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {types.map((tp) => (
                <SelectItem key={tp.id} value={tp.id}>
                  {tp.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      <div className="grid gap-2">
        <Label htmlFor="profile-name">{t('dnsProviders.editor.name')}</Label>
        <Input
          id="profile-name"
          placeholder={t('dnsProviders.editor.namePlaceholder')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </div>

      {(type?.fields ?? []).map((field) => (
        <div key={field.env} className="grid gap-2">
          <Label htmlFor={`profile-cred-${field.env}`}>{field.label}</Label>
          <Input
            id={`profile-cred-${field.env}`}
            type="password"
            autoComplete="new-password"
            placeholder={isEdit ? '••••••••' : ''}
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
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          {t('common.cancel')}
        </Button>
        <Button type="submit" disabled={mutation.isPending || !canSave}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {isEdit ? t('common.save') : t('dnsProviders.editor.create')}
        </Button>
      </DialogFooter>
    </form>
  )
}

function DeleteProfileDialog({
  profile,
  onOpenChange,
}: {
  profile: DnsCredentialProfile | null
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteDnsCredential(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['dns-credentials'] })
      toast({ variant: 'success', title: t('dnsProviders.deleted') })
      onOpenChange(false)
    },
    onError: (err) => {
      toast({
        variant: 'destructive',
        title: t('dnsProviders.actionFailed'),
        description: err instanceof ApiError ? err.message : t('common.error'),
      })
    },
  })

  return (
    <Dialog open={profile !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('dnsProviders.deleteDialog.title')}</DialogTitle>
          <DialogDescription>
            {t('dnsProviders.deleteDialog.body', { name: profile?.name ?? '' })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={deleteMutation.isPending}
          >
            {t('common.cancel')}
          </Button>
          <Button
            variant="destructive"
            onClick={() => profile !== null && deleteMutation.mutate(profile.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('dnsProviders.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

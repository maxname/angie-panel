import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Loader2, Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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
import { api, ApiError, type Role, type User } from '@/lib/api'
import { toast } from '@/lib/toast'
import { useMe } from '@/lib/use-me'

export function UsersPage() {
  const { t } = useTranslation()
  const [createOpen, setCreateOpen] = useState(false)

  const usersQuery = useQuery({ queryKey: ['users'], queryFn: () => api.listUsers() })
  const { data: me } = useMe()

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t('users.title')}
          </h1>
          <p className="text-sm text-muted-foreground">{t('users.subtitle')}</p>
        </div>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus aria-hidden="true" />
          {t('users.add')}
        </Button>
      </div>

      {usersQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : usersQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('users.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void usersQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('users.table.email')}</TableHead>
                  <TableHead>{t('users.table.role')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('users.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {usersQuery.data.users.map((user) => (
                  <UserRow key={user.id} user={user} selfEmail={me?.email} />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <ChangePasswordCard />

      <CreateUserDialog open={createOpen} onOpenChange={setCreateOpen} />
    </div>
  )
}

function UserRow({ user, selfEmail }: { user: User; selfEmail?: string }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const isSelf = user.email === selfEmail

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['users'] })
  const onError = (error: unknown) =>
    toast({
      variant: 'destructive',
      title: t('users.actionFailed'),
      description: error instanceof ApiError ? error.message : t('common.error'),
    })

  const roleMutation = useMutation({
    mutationFn: (role: Role) => api.setUserRole(user.id, role),
    onSuccess: () => void invalidate(),
    onError,
  })
  const deleteMutation = useMutation({
    mutationFn: () => api.deleteUser(user.id),
    onSuccess: () => void invalidate(),
    onError,
  })

  return (
    <TableRow>
      <TableCell className="font-medium">
        {user.email}
        {isSelf && (
          <span className="ml-2 text-xs text-muted-foreground">
            {t('users.you')}
          </span>
        )}
      </TableCell>
      <TableCell>
        <Select
          value={user.role}
          onValueChange={(value) => roleMutation.mutate(value as Role)}
          disabled={roleMutation.isPending}
        >
          <SelectTrigger className="h-8 w-32" aria-label={t('users.table.role')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="admin">{t('users.roles.admin')}</SelectItem>
            <SelectItem value="viewer">{t('users.roles.viewer')}</SelectItem>
          </SelectContent>
        </Select>
      </TableCell>
      <TableCell className="text-right">
        <Button
          variant="ghost"
          size="sm"
          className="text-destructive"
          disabled={isSelf || deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          {deleteMutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('users.delete')}
        </Button>
      </TableCell>
    </TableRow>
  )
}

function CreateUserDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<Role>('viewer')
  const [error, setError] = useState<string | null>(null)

  const mutation = useMutation({
    mutationFn: () => api.createUser({ email, password, role }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      onOpenChange(false)
      setEmail('')
      setPassword('')
      setRole('viewer')
      setError(null)
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : t('common.error')),
  })

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError(null)
    mutation.mutate()
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('users.create.title')}</DialogTitle>
          <DialogDescription>{t('users.create.description')}</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
          <div className="space-y-2">
            <Label htmlFor="new-user-email">{t('users.create.email')}</Label>
            <Input
              id="new-user-email"
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-user-password">{t('users.create.password')}</Label>
            <Input
              id="new-user-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-user-role">{t('users.create.role')}</Label>
            <Select value={role} onValueChange={(v) => setRole(v as Role)}>
              <SelectTrigger id="new-user-role">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="admin">{t('users.roles.admin')}</SelectItem>
                <SelectItem value="viewer">{t('users.roles.viewer')}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {error !== null && (
            <Alert variant="destructive">
              <AlertTitle>{t('users.create.failed')}</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={mutation.isPending}
            >
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending && <Loader2 className="animate-spin" aria-hidden="true" />}
              {t('users.create.submit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** Any operator (incl. viewers) can change their own password. */
export function ChangePasswordCard() {
  const { t } = useTranslation()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [error, setError] = useState<string | null>(null)

  const mutation = useMutation({
    mutationFn: () =>
      api.changeOwnPassword({ current_password: current, new_password: next }),
    onSuccess: () => {
      setCurrent('')
      setNext('')
      setError(null)
      toast({ variant: 'success', title: t('users.password.changed') })
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : t('common.error')),
  })

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError(null)
    mutation.mutate()
  }

  return (
    <Card>
      <CardContent className="space-y-4">
        <div className="space-y-1">
          <h2 className="text-lg font-medium">{t('users.password.title')}</h2>
          <p className="text-sm text-muted-foreground">
            {t('users.password.description')}
          </p>
        </div>
        <form className="flex flex-col gap-4 sm:max-w-sm" onSubmit={submit} noValidate>
          <div className="space-y-2">
            <Label htmlFor="current-password">{t('users.password.current')}</Label>
            <Input
              id="current-password"
              type="password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-password">{t('users.password.new')}</Label>
            <Input
              id="new-password"
              type="password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
            />
          </div>
          {error !== null && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
          <div>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending && <Loader2 className="animate-spin" aria-hidden="true" />}
              {t('users.password.submit')}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}

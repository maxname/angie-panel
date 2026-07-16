import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Info, Loader2, MoreHorizontal, Plus, X } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
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
import { Switch } from '@/components/ui/switch'
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
  type AccessList,
  type AccessListDirective,
  type AccessListInput,
  type AccessListSatisfy,
  type AccessListUserInput,
} from '@/lib/api'
import { useIsDirty } from '@/lib/use-dirty'
import { toast } from '@/lib/toast'

export function AccessListsPage() {
  const { t } = useTranslation()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<AccessList | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<AccessList | null>(null)

  const listsQuery = useQuery({
    queryKey: ['access-lists'],
    queryFn: () => api.listAccessLists(),
  })

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const openEdit = (list: AccessList) => {
    setEditing(list)
    setEditorOpen(true)
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('accessLists.title')}
        </h1>
        <Button onClick={openCreate}>
          <Plus aria-hidden="true" />
          {t('accessLists.add')}
        </Button>
      </div>

      <Alert variant="info">
        <Info aria-hidden="true" />
        <AlertDescription>{t('accessLists.info.body')}</AlertDescription>
      </Alert>

      {listsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : listsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('accessLists.loadFailed')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void listsQuery.refetch()}
            >
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : listsQuery.data.access_lists.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">
              {t('accessLists.empty')}
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('accessLists.table.name')}</TableHead>
                  <TableHead>{t('accessLists.table.summary')}</TableHead>
                  <TableHead>{t('accessLists.table.match')}</TableHead>
                  <TableHead>{t('accessLists.table.passAuth')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">
                      {t('accessLists.table.actions')}
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {listsQuery.data.access_lists.map((list) => (
                  <AccessListRow
                    key={list.id}
                    list={list}
                    onEdit={() => openEdit(list)}
                    onDelete={() => setDeleteTarget(list)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <AccessListEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        list={editing}
      />
      <DeleteAccessListDialog
        list={deleteTarget}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

interface AccessListRowProps {
  list: AccessList
  onEdit: () => void
  onDelete: () => void
}

function AccessListRow({ list, onEdit, onDelete }: AccessListRowProps) {
  const { t } = useTranslation()

  return (
    <TableRow>
      <TableCell>
        <span className="font-mono text-xs">{list.name}</span>
      </TableCell>
      <TableCell className="whitespace-nowrap text-muted-foreground">
        {t('accessLists.table.counts', {
          users: list.users.length,
          clients: list.clients.length,
        })}
      </TableCell>
      <TableCell>
        <Badge variant="outline">
          {t(`accessLists.satisfy.${list.satisfy}`)}
        </Badge>
      </TableCell>
      <TableCell>
        {list.pass_auth ? (
          <Badge className="bg-sky-600/15 text-sky-700 dark:bg-sky-400/15 dark:text-sky-400">
            {t('accessLists.passAuthBadge')}
          </Badge>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell className="text-right">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('accessLists.table.actions')}
            >
              <MoreHorizontal aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              {t('accessLists.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('accessLists.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

interface DeleteAccessListDialogProps {
  list: AccessList | null
  onOpenChange: (open: boolean) => void
}

export function DeleteAccessListDialog({
  list,
  onOpenChange,
}: DeleteAccessListDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteAccessList(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['access-lists'] })
      toast({
        title: t('accessLists.unappliedTitle'),
        description: t('accessLists.unappliedBody'),
      })
      onOpenChange(false)
    },
  })

  // 409 access_list_in_use carries a server message that lists the hosts still
  // referencing this list — show it verbatim.
  const serverError =
    deleteMutation.isError && deleteMutation.error instanceof ApiError
      ? deleteMutation.error.message
      : deleteMutation.isError
        ? t('common.error')
        : null

  return (
    <Dialog
      open={list !== null}
      onOpenChange={(open) => {
        if (!open) {
          deleteMutation.reset()
        }
        onOpenChange(open)
      }}
    >
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('accessLists.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('accessLists.delete.body', { name: list?.name ?? '' })}
          </DialogDescription>
        </DialogHeader>
        {serverError !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('accessLists.delete.failed')}</AlertTitle>
            <AlertDescription>{serverError}</AlertDescription>
          </Alert>
        )}
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
            onClick={() => list !== null && deleteMutation.mutate(list.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('accessLists.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface AccessListEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The access list being edited, or null when creating a new one. */
  list: AccessList | null
}

function AccessListEditorDialog({
  open,
  onOpenChange,
  list,
}: AccessListEditorDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {list === null
              ? t('accessLists.editor.createTitle')
              : t('accessLists.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>
            {t('accessLists.editor.description')}
          </DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target list changes so state resets. */}
        <AccessListEditorForm
          key={list?.id ?? 'new'}
          list={list}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface UserDraft {
  username: string
  password: string
  /** True when the row came from the server (edit) → its password is kept if left blank. */
  existing: boolean
}

interface ClientDraft {
  directive: AccessListDirective
  address: string
}

interface FormState {
  name: string
  satisfy: AccessListSatisfy
  pass_auth: boolean
  users: UserDraft[]
  clients: ClientDraft[]
}

function initialState(list: AccessList | null): FormState {
  if (list === null) {
    return {
      name: '',
      satisfy: 'all',
      pass_auth: false,
      users: [],
      clients: [],
    }
  }
  return {
    name: list.name,
    satisfy: list.satisfy,
    pass_auth: list.pass_auth,
    users: list.users.map((user) => ({
      username: user.username,
      password: '',
      existing: true,
    })),
    clients: list.clients.map((client) => ({
      directive: client.directive,
      address: client.address,
    })),
  }
}

interface FieldErrors {
  name?: string
  form?: string
}

interface AccessListEditorFormProps {
  list: AccessList | null
  onDone: () => void
}

export function AccessListEditorForm({
  list,
  onDone,
}: AccessListEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  // Keep the opening snapshot so the submit can tell whether there is
  // anything to save.
  const [initialForm] = useState<FormState>(() => initialState(list))
  const [form, setForm] = useState<FormState>(initialForm)
  // Only an edit can be "unchanged". A create form starts pristine by
  // definition, and its Save is what tells you which fields are missing.
  const canSave = useIsDirty(form, initialForm) || list === null
  const [clientErrors, setClientErrors] = useState<FieldErrors>({})

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  const mutation = useMutation({
    mutationFn: (input: AccessListInput) =>
      list === null
        ? api.createAccessList(input)
        : api.updateAccessList(list.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['access-lists'] })
      toast({
        title: t('accessLists.editor.savedToastTitle'),
        description: t('accessLists.editor.savedToastBody'),
      })
      onDone()
    },
  })

  // Map the server's typed error codes onto the name field or a form-level alert.
  const serverErrors = useMemo<FieldErrors>(() => {
    if (!mutation.isError) {
      return {}
    }
    if (mutation.error instanceof ApiError) {
      const { code, message } = mutation.error
      if (code === 'invalid_name') {
        return { name: message }
      }
      return { form: message }
    }
    return { form: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const errors: FieldErrors = {
    name: clientErrors.name ?? serverErrors.name,
    form: clientErrors.form ?? serverErrors.form,
  }

  const addUser = () =>
    patch({
      users: [...form.users, { username: '', password: '', existing: false }],
    })

  const updateUser = (index: number, partial: Partial<UserDraft>) =>
    patch({
      users: form.users.map((user, i) =>
        i === index ? { ...user, ...partial } : user,
      ),
    })

  const removeUser = (index: number) =>
    patch({ users: form.users.filter((_, i) => i !== index) })

  const addClient = () =>
    patch({
      clients: [...form.clients, { directive: 'allow', address: '' }],
    })

  const updateClient = (index: number, partial: Partial<ClientDraft>) =>
    patch({
      clients: form.clients.map((client, i) =>
        i === index ? { ...client, ...partial } : client,
      ),
    })

  const removeClient = (index: number) =>
    patch({ clients: form.clients.filter((_, i) => i !== index) })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    if (form.name.trim() === '') {
      setClientErrors({ name: t('accessLists.editor.noName') })
      return
    }

    const users = form.users.filter((user) => user.username.trim() !== '')
    const clients = form.clients.filter((client) => client.address.trim() !== '')
    if (users.length === 0 && clients.length === 0) {
      setClientErrors({ form: t('accessLists.editor.empty') })
      return
    }
    setClientErrors({})

    const input: AccessListInput = {
      name: form.name.trim(),
      satisfy: form.satisfy,
      pass_auth: form.pass_auth,
      // Only send `password` when the user typed one; omitting it keeps the
      // existing password on edit. New users with no password are rejected by
      // the server (password_required), which surfaces as a form-level error.
      users: users.map((user): AccessListUserInput => {
        const username = user.username.trim()
        return user.password === '' ? { username } : { username, password: user.password }
      }),
      clients: clients.map((client) => ({
        directive: client.directive,
        address: client.address.trim(),
      })),
    }
    mutation.mutate(input)
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      <div className="space-y-2">
        <Label htmlFor="al-name">{t('accessLists.editor.name')}</Label>
        <Input
          id="al-name"
          value={form.name}
          spellCheck={false}
          autoComplete="off"
          placeholder={t('accessLists.editor.namePlaceholder')}
          onChange={(event) => {
            patch({ name: event.target.value })
            setClientErrors((prev) => ({ ...prev, name: undefined }))
          }}
        />
        {errors.name !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.name}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor="al-satisfy">{t('accessLists.editor.satisfy')}</Label>
        <Select
          value={form.satisfy}
          onValueChange={(value) => patch({ satisfy: value as AccessListSatisfy })}
        >
          <SelectTrigger id="al-satisfy">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">
              {t('accessLists.editor.satisfyAll')}
            </SelectItem>
            <SelectItem value="any">
              {t('accessLists.editor.satisfyAny')}
            </SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          {t('accessLists.editor.satisfyHelp')}
        </p>
      </div>

      <div className="space-y-2 rounded-lg border p-3">
        <div className="flex items-center justify-between gap-4">
          <Label htmlFor="al-pass-auth" className="font-normal">
            {t('accessLists.editor.passAuth')}
          </Label>
          <Switch
            id="al-pass-auth"
            checked={form.pass_auth}
            onCheckedChange={(checked) => patch({ pass_auth: checked })}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          {t('accessLists.editor.passAuthHelp')}
        </p>
      </div>

      <fieldset className="space-y-3">
        <legend className="text-sm font-medium">
          {t('accessLists.editor.users')}
        </legend>
        {form.users.length === 0 && (
          <p className="text-sm text-muted-foreground">
            {t('accessLists.editor.usersEmpty')}
          </p>
        )}
        {form.users.map((user, index) => (
          <div key={index} className="flex items-end gap-2">
            <div className="flex-1 space-y-1">
              <Label htmlFor={`al-user-name-${index}`} className="text-xs">
                {t('accessLists.editor.username')}
              </Label>
              <Input
                id={`al-user-name-${index}`}
                value={user.username}
                spellCheck={false}
                autoComplete="off"
                onChange={(event) =>
                  updateUser(index, { username: event.target.value })
                }
              />
            </div>
            <div className="flex-1 space-y-1">
              <Label htmlFor={`al-user-pass-${index}`} className="text-xs">
                {t('accessLists.editor.password')}
              </Label>
              <Input
                id={`al-user-pass-${index}`}
                type="password"
                value={user.password}
                autoComplete="new-password"
                placeholder={
                  user.existing
                    ? t('accessLists.editor.passwordUnchanged')
                    : undefined
                }
                onChange={(event) =>
                  updateUser(index, { password: event.target.value })
                }
              />
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={() => removeUser(index)}
              aria-label={t('accessLists.editor.removeUser')}
            >
              <X aria-hidden="true" />
            </Button>
          </div>
        ))}
        <Button type="button" variant="outline" onClick={addUser}>
          <Plus aria-hidden="true" />
          {t('accessLists.editor.addUser')}
        </Button>
      </fieldset>

      <fieldset className="space-y-3">
        <legend className="text-sm font-medium">
          {t('accessLists.editor.clients')}
        </legend>
        {form.clients.length === 0 && (
          <p className="text-sm text-muted-foreground">
            {t('accessLists.editor.clientsEmpty')}
          </p>
        )}
        {form.clients.map((client, index) => (
          <div key={index} className="flex items-end gap-2">
            <div className="w-32 space-y-1">
              <Label htmlFor={`al-client-dir-${index}`} className="text-xs">
                {t('accessLists.editor.directive')}
              </Label>
              <Select
                value={client.directive}
                onValueChange={(value) =>
                  updateClient(index, {
                    directive: value as AccessListDirective,
                  })
                }
              >
                <SelectTrigger id={`al-client-dir-${index}`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="allow">
                    {t('accessLists.editor.directiveAllow')}
                  </SelectItem>
                  <SelectItem value="deny">
                    {t('accessLists.editor.directiveDeny')}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex-1 space-y-1">
              <Label htmlFor={`al-client-addr-${index}`} className="text-xs">
                {t('accessLists.editor.address')}
              </Label>
              <Input
                id={`al-client-addr-${index}`}
                value={client.address}
                spellCheck={false}
                autoComplete="off"
                placeholder={t('accessLists.editor.addressPlaceholder')}
                onChange={(event) =>
                  updateClient(index, { address: event.target.value })
                }
              />
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={() => removeClient(index)}
              aria-label={t('accessLists.editor.removeClient')}
            >
              <X aria-hidden="true" />
            </Button>
          </div>
        ))}
        <Button type="button" variant="outline" onClick={addClient}>
          <Plus aria-hidden="true" />
          {t('accessLists.editor.addClient')}
        </Button>
      </fieldset>

      {errors.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('accessLists.editor.saveFailed')}</AlertTitle>
          <AlertDescription>{errors.form}</AlertDescription>
        </Alert>
      )}

      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onDone}
          disabled={mutation.isPending}
        >
          {t('common.cancel')}
        </Button>
        <Button type="submit" disabled={mutation.isPending || !canSave}>
          {mutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {t('common.save')}
        </Button>
      </DialogFooter>
    </form>
  )
}

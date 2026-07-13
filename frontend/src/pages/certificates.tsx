import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query'
import type { TFunction } from 'i18next'
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
  type AcmeChallenge,
  type AcmeKeyType,
  type AcmeStatus,
  type Cert,
  type CertInput,
  type CertPrecheck,
  type DnsCredentialProfile,
} from '@/lib/api'
import { isValidDomain } from '@/lib/domain'
import { toast } from '@/lib/toast'

/** Create a fresh cert, or edit an existing one, through the same wizard. */
type WizardTarget = { mode: 'create' } | { mode: 'edit'; cert: Cert }

export function CertificatesPage() {
  const { t } = useTranslation()
  const [wizard, setWizard] = useState<WizardTarget | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Cert | null>(null)

  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t('certificates.title')}
        </h1>
        <Button onClick={() => setWizard({ mode: 'create' })}>
          <Plus aria-hidden="true" />
          {t('certificates.add')}
        </Button>
      </div>

      <Alert variant="info">
        <Info aria-hidden="true" />
        <AlertTitle>{t('certificates.info.title')}</AlertTitle>
        <AlertDescription>{t('certificates.info.body')}</AlertDescription>
      </Alert>

      {certsQuery.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('common.loading')}
        </div>
      ) : certsQuery.isError ? (
        <Card>
          <CardContent className="flex flex-col items-start gap-3">
            <p className="text-sm text-destructive" role="alert">
              {t('certificates.loadFailed')}
            </p>
            <Button variant="outline" size="sm" onClick={() => void certsQuery.refetch()}>
              {t('common.retry')}
            </Button>
          </CardContent>
        </Card>
      ) : certsQuery.data.certificates.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <p className="text-sm text-muted-foreground">{t('certificates.empty')}</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="p-0">
          <CardContent className="px-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('certificates.table.domains')}</TableHead>
                  <TableHead>{t('certificates.table.challenge')}</TableHead>
                  <TableHead>{t('certificates.table.keyType')}</TableHead>
                  <TableHead>{t('certificates.table.environment')}</TableHead>
                  <TableHead>{t('certificates.table.status')}</TableHead>
                  <TableHead>{t('certificates.table.created')}</TableHead>
                  <TableHead className="w-0 text-right">
                    <span className="sr-only">{t('certificates.table.actions')}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {certsQuery.data.certificates.map((cert) => (
                  <CertRow
                    key={cert.id}
                    cert={cert}
                    onEdit={() => setWizard({ mode: 'edit', cert })}
                    onDelete={() => setDeleteTarget(cert)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      <CertWizardDialog
        target={wizard}
        onOpenChange={(open) => {
          if (!open) {
            setWizard(null)
          }
        }}
      />
      <DeleteCertDialog
        cert={deleteTarget}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTarget(null)
          }
        }}
      />
    </div>
  )
}

interface CertRowProps {
  cert: Cert
  onEdit: () => void
  onDelete: () => void
}

function CertRow({ cert, onEdit, onDelete }: CertRowProps) {
  const { t, i18n } = useTranslation()
  const created = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
  }).format(new Date(cert.created_at * 1000))

  return (
    <TableRow>
      <TableCell>
        <div className="space-y-1">
          <div className="flex flex-wrap gap-1">
            {cert.domains.map((domain) => (
              <Badge key={domain} variant="secondary">
                {domain}
              </Badge>
            ))}
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {cert.name}
          </span>
        </div>
      </TableCell>
      <TableCell>
        <Badge variant="outline">
          {t(`certificates.challenge.${cert.challenge}`)}
        </Badge>
      </TableCell>
      <TableCell className="text-muted-foreground">
        {t(`certificates.keyType.${cert.key_type}`)}
      </TableCell>
      <TableCell>
        {cert.staging ? (
          <Badge className="bg-amber-600/15 text-amber-700 dark:bg-amber-400/15 dark:text-amber-400">
            {t('certificates.environment.staging')}
          </Badge>
        ) : (
          <span className="text-muted-foreground">
            {t('certificates.environment.production')}
          </span>
        )}
      </TableCell>
      <TableCell>
        <StatusPill status={cert.status} />
      </TableCell>
      <TableCell className="whitespace-nowrap text-muted-foreground">{created}</TableCell>
      <TableCell className="text-right">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t('certificates.table.actions')}
            >
              <MoreHorizontal aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              {t('certificates.actions.edit')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {t('certificates.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </TableRow>
  )
}

type StatusKind = 'unknown' | 'issued' | 'pending' | 'error'

interface DerivedStatus {
  kind: StatusKind
  label: string
  hint?: string
}

/**
 * Turns Angie's (possibly-null, possibly-partial) ACME status into a pill:
 *   null                    → Unknown  (the status API is unreachable)
 *   certificate === "valid" → Issued
 *   any other state/cert    → that value, red for error-ish words else amber
 *   object with no signal   → Pending
 */
function deriveStatus(status: AcmeStatus | null, t: TFunction): DerivedStatus {
  if (status === null) {
    return {
      kind: 'unknown',
      label: t('certificates.status.unknown'),
      hint: t('certificates.status.unknownHint'),
    }
  }
  if (status.certificate === 'valid') {
    return { kind: 'issued', label: t('certificates.status.issued'), hint: status.details }
  }
  const raw = status.state ?? status.certificate
  if (raw !== undefined && raw !== '') {
    const lowered = raw.toLowerCase()
    const isError = ['expired', 'invalid', 'error', 'fail', 'revoked'].some((word) =>
      lowered.includes(word),
    )
    return {
      kind: isError ? 'error' : 'pending',
      label: raw.charAt(0).toUpperCase() + raw.slice(1),
      hint: status.details,
    }
  }
  return { kind: 'pending', label: t('certificates.status.pending'), hint: status.details }
}

export function StatusPill({ status }: { status: AcmeStatus | null }) {
  const { t } = useTranslation()
  const derived = deriveStatus(status, t)

  if (derived.kind === 'issued') {
    return (
      <Badge
        title={derived.hint}
        className="bg-emerald-600/15 text-emerald-700 dark:bg-emerald-400/15 dark:text-emerald-400"
      >
        {derived.label}
      </Badge>
    )
  }
  if (derived.kind === 'pending') {
    return (
      <Badge
        title={derived.hint}
        className="bg-amber-600/15 text-amber-700 dark:bg-amber-400/15 dark:text-amber-400"
      >
        {derived.label}
      </Badge>
    )
  }
  if (derived.kind === 'error') {
    return (
      <Badge variant="destructive" title={derived.hint}>
        {derived.label}
      </Badge>
    )
  }
  return (
    <Badge variant="outline" className="text-muted-foreground" title={derived.hint}>
      {derived.label}
    </Badge>
  )
}

interface DeleteCertDialogProps {
  cert: Cert | null
  onOpenChange: (open: boolean) => void
}

function DeleteCertDialog({ cert, onOpenChange }: DeleteCertDialogProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  const deleteMutation = useMutation({
    mutationFn: (id: number) => api.deleteCertificate(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['certificates'] })
      onOpenChange(false)
    },
  })

  // 409 cert_in_use carries a server message that lists the hosts — show it verbatim.
  const serverError =
    deleteMutation.isError && deleteMutation.error instanceof ApiError
      ? deleteMutation.error.message
      : deleteMutation.isError
        ? t('common.error')
        : null

  return (
    <Dialog
      open={cert !== null}
      onOpenChange={(open) => {
        if (!open) {
          deleteMutation.reset()
        }
        onOpenChange(open)
      }}
    >
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('certificates.delete.title')}</DialogTitle>
          <DialogDescription>
            {t('certificates.delete.body', { name: cert?.name ?? '' })}
          </DialogDescription>
        </DialogHeader>
        {serverError !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('certificates.delete.failed')}</AlertTitle>
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
            onClick={() => cert !== null && deleteMutation.mutate(cert.id)}
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('certificates.actions.delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface CertWizardDialogProps {
  target: WizardTarget | null
  onOpenChange: (open: boolean) => void
}

function CertWizardDialog({ target, onOpenChange }: CertWizardDialogProps) {
  const { t } = useTranslation()
  const editing = target?.mode === 'edit'

  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {editing ? t('certificates.wizard.editTitle') : t('certificates.wizard.title')}
          </DialogTitle>
          <DialogDescription>
            {editing
              ? t('certificates.wizard.editDescription')
              : t('certificates.wizard.description')}
          </DialogDescription>
        </DialogHeader>
        {/* Radix unmounts content on close, so state resets on each open — the
            form seeds from `cert` on mount. */}
        <CertWizardForm
          cert={target?.mode === 'edit' ? target.cert : undefined}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

interface WizardState {
  name: string
  domains: string[]
  challenge: AcmeChallenge
  key_type: AcmeKeyType
  email: string
  staging: boolean
  /** For DNS-01: a provider id = automatic via that provider's API; null = Angie
   *  self-answers (NS delegation). */
  dns_provider: string | null
}

const CHALLENGE_OPTIONS: { value: AcmeChallenge; labelKey: string }[] = [
  { value: 'http', labelKey: 'certificates.wizard.challengeHttp' },
  { value: 'alpn', labelKey: 'certificates.wizard.challengeAlpn' },
  { value: 'dns', labelKey: 'certificates.wizard.challengeDns' },
]

interface WizardFieldErrors {
  name?: string
  domains?: string
  challenge?: string
  email?: string
  form?: string
}

export function CertWizardForm({
  cert,
  onDone,
}: {
  cert?: Cert
  onDone: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const editing = cert !== undefined

  const [form, setForm] = useState<WizardState>(() =>
    cert
      ? {
          name: cert.name,
          domains: cert.domains,
          challenge: cert.challenge,
          key_type: cert.key_type,
          email: cert.email ?? '',
          staging: cert.staging,
          dns_provider: cert.dns_provider,
        }
      : {
          name: '',
          domains: [],
          challenge: 'http',
          key_type: 'ecdsa',
          email: '',
          staging: false,
          dns_provider: null,
        },
  )
  const profilesQuery = useQuery({
    queryKey: ['dns-credentials'],
    queryFn: () => api.listDnsCredentials(),
  })
  const profiles = profilesQuery.data?.credentials ?? []
  const [domainDraft, setDomainDraft] = useState('')
  const [domainError, setDomainError] = useState<string | null>(null)
  const [clientErrors, setClientErrors] = useState<WizardFieldErrors>({})
  const [created, setCreated] = useState<Cert | null>(null)

  const patch = (partial: Partial<WizardState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  // A wildcard domain (leading "*.") can only be validated over DNS-01, so we
  // force the challenge and disable the other two options while one is present.
  const hasWildcard = form.domains.some((domain) => domain.startsWith('*.'))
  const effectiveChallenge: AcmeChallenge = hasWildcard ? 'dns' : form.challenge
  const usingDns = effectiveChallenge === 'dns'
  // The provider profile only applies to DNS-01; ignore any stray choice
  // otherwise. dns_provider holds a credential-profile id (as a string).
  const effectiveDnsProvider: string | null = usingDns ? form.dns_provider : null
  const selectedProfile: DnsCredentialProfile | undefined = profiles.find(
    (p) => String(p.id) === form.dns_provider,
  )

  const precheckMutation = useMutation({
    mutationFn: (id: number) => api.precheckCertificate(id),
  })

  const saveMutation = useMutation({
    mutationFn: (input: CertInput) =>
      editing ? api.updateCertificate(cert.id, input) : api.createCertificate(input),
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ['certificates'] })
      toast({
        title: editing
          ? t('certificates.wizard.updatedToastTitle')
          : t('certificates.wizard.createdToastTitle'),
        description: editing
          ? t('certificates.wizard.updatedToastBody')
          : t('certificates.wizard.createdToastBody'),
      })
      if (saved.challenge === 'dns' && saved.dns_provider === null) {
        // Self-answered DNS-01: show the NS-delegation records the user must
        // create. Provider certs need none — the hook does it — so we just close.
        setCreated(saved)
        precheckMutation.mutate(saved.id)
      } else {
        onDone()
      }
    },
  })

  const serverErrors = useMemo<WizardFieldErrors>(() => {
    if (!saveMutation.isError) {
      return {}
    }
    if (saveMutation.error instanceof ApiError) {
      const { code, message } = saveMutation.error
      switch (code) {
        case 'invalid_cert_name':
        case 'cert_name_taken':
          return { name: message }
        case 'invalid_domain':
          return { domains: message }
        case 'wildcard_needs_dns':
          return { challenge: message }
        case 'invalid_email':
          return { email: message }
        default:
          return { form: message }
      }
    }
    return { form: t('common.error') }
  }, [saveMutation.isError, saveMutation.error, t])

  const errors: WizardFieldErrors = {
    name: clientErrors.name ?? serverErrors.name,
    domains: clientErrors.domains ?? serverErrors.domains,
    challenge: serverErrors.challenge,
    email: serverErrors.email,
    form: serverErrors.form,
  }

  const addDomain = () => {
    const candidate = domainDraft.trim().toLowerCase()
    if (candidate === '') {
      return
    }
    if (!isValidDomain(candidate)) {
      setDomainError(t('certificates.wizard.invalidDomain'))
      return
    }
    if (form.domains.includes(candidate)) {
      setDomainError(t('certificates.wizard.duplicateDomain'))
      return
    }
    patch({ domains: [...form.domains, candidate] })
    setDomainDraft('')
    setDomainError(null)
  }

  const removeDomain = (domain: string) =>
    patch({ domains: form.domains.filter((item) => item !== domain) })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const nextErrors: WizardFieldErrors = {}
    if (editing && form.name.trim() === '') {
      nextErrors.name = t('certificates.wizard.noName')
    }
    if (form.domains.length === 0) {
      nextErrors.domains = t('certificates.wizard.noDomains')
    }
    if (nextErrors.name !== undefined || nextErrors.domains !== undefined) {
      setClientErrors(nextErrors)
      return
    }
    setClientErrors({})

    const input: CertInput = {
      name: form.name.trim(),
      domains: form.domains,
      challenge: effectiveChallenge,
      key_type: form.key_type,
      email: form.email.trim() === '' ? null : form.email.trim(),
      staging: form.staging,
      dns_provider: effectiveDnsProvider,
    }
    saveMutation.mutate(input)
  }

  if (created !== null) {
    return (
      <PrecheckPanel
        query={precheckMutation}
        onRetry={() => precheckMutation.mutate(created.id)}
        onDone={onDone}
      />
    )
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={handleSubmit} noValidate>
      {editing && (
        <Alert variant="info">
          <Info aria-hidden="true" />
          <AlertDescription>{t('certificates.wizard.editNote')}</AlertDescription>
        </Alert>
      )}
      {/* Name is the acme_client identifier. On create it's auto-derived from
          the first domain (NPM has no name field); only surfaced when editing. */}
      {editing && (
        <div className="space-y-2">
          <Label htmlFor="cert-name">{t('certificates.wizard.name')}</Label>
          <Input
            id="cert-name"
            value={form.name}
            spellCheck={false}
            autoComplete="off"
            placeholder="my_site"
            onChange={(event) => {
              patch({ name: event.target.value })
              setClientErrors((prev) => ({ ...prev, name: undefined }))
            }}
          />
          <p className="text-xs text-muted-foreground">
            {t('certificates.wizard.nameHelp')}
          </p>
          {errors.name !== undefined && (
            <p role="alert" className="text-sm text-destructive">
              {errors.name}
            </p>
          )}
        </div>
      )}

      <div className="space-y-2">
        <Label htmlFor="cert-domain-input">{t('certificates.wizard.domains')}</Label>
        <div className="flex flex-wrap gap-1.5">
          {form.domains.map((domain) => (
            <span
              key={domain}
              className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-sm"
            >
              {domain}
              <button
                type="button"
                onClick={() => removeDomain(domain)}
                className="text-muted-foreground hover:text-foreground"
                aria-label={t('certificates.wizard.removeDomain', { domain })}
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
        <div className="flex gap-2">
          <Input
            id="cert-domain-input"
            value={domainDraft}
            placeholder={t('certificates.wizard.domainPlaceholder')}
            onChange={(event) => {
              setDomainDraft(event.target.value)
              setDomainError(null)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ',') {
                event.preventDefault()
                addDomain()
              }
            }}
          />
          <Button type="button" variant="outline" onClick={addDomain}>
            <Plus aria-hidden="true" />
            {t('certificates.wizard.addDomain')}
          </Button>
        </div>
        {domainError !== null && (
          <p role="alert" className="text-sm text-destructive">
            {domainError}
          </p>
        )}
        {errors.domains !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.domains}
          </p>
        )}
      </div>

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium">
          {t('certificates.wizard.challenge')}
        </legend>
        <div className="space-y-2 rounded-lg border p-3">
          {CHALLENGE_OPTIONS.map((option) => {
            const id = `cert-challenge-${option.value}`
            const disabled = hasWildcard && option.value !== 'dns'
            return (
              <label
                key={option.value}
                htmlFor={id}
                className={`flex items-start gap-3 ${
                  disabled ? 'opacity-50' : 'cursor-pointer'
                }`}
              >
                <input
                  type="radio"
                  id={id}
                  name="cert-challenge"
                  className="mt-0.5 size-4 accent-primary"
                  value={option.value}
                  checked={effectiveChallenge === option.value}
                  disabled={disabled}
                  onChange={() => patch({ challenge: option.value })}
                />
                <span className="text-sm">{t(option.labelKey)}</span>
              </label>
            )
          })}
        </div>
        {hasWildcard && (
          <p className="text-xs text-muted-foreground">
            {t('certificates.wizard.wildcardNote')}
          </p>
        )}
        {errors.challenge !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {errors.challenge}
          </p>
        )}
      </fieldset>

      {usingDns && (
        <fieldset className="space-y-2">
          <legend className="text-sm font-medium">
            {t('certificates.wizard.dnsMethod')}
          </legend>
          <div className="space-y-2 rounded-lg border p-3">
            <label
              htmlFor="cert-dns-self"
              className="flex cursor-pointer items-start gap-3"
            >
              <input
                type="radio"
                id="cert-dns-self"
                name="cert-dns-method"
                className="mt-0.5 size-4 accent-primary"
                checked={form.dns_provider === null}
                onChange={() => patch({ dns_provider: null })}
              />
              <span className="text-sm">
                {t('certificates.wizard.dnsMethodSelf')}
                <span className="block text-xs text-muted-foreground">
                  {t('certificates.wizard.dnsMethodSelfHint')}
                </span>
              </span>
            </label>
            <label
              htmlFor="cert-dns-provider"
              className="flex cursor-pointer items-start gap-3"
            >
              <input
                type="radio"
                id="cert-dns-provider"
                name="cert-dns-method"
                className="mt-0.5 size-4 accent-primary"
                checked={form.dns_provider !== null}
                disabled={profiles.length === 0}
                onChange={() =>
                  patch({
                    dns_provider:
                      form.dns_provider ??
                      (profiles[0] ? String(profiles[0].id) : null),
                  })
                }
              />
              <span className="text-sm">
                {t('certificates.wizard.dnsMethodProvider')}
                <span className="block text-xs text-muted-foreground">
                  {profiles.length === 0
                    ? t('certificates.wizard.noProviders')
                    : t('certificates.wizard.dnsMethodProviderHint')}
                </span>
              </span>
            </label>
          </div>
          {form.dns_provider !== null && (
            <div className="space-y-2 pl-7">
              <Label htmlFor="cert-dns-provider-select">
                {t('certificates.wizard.provider')}
              </Label>
              <Select
                value={form.dns_provider}
                onValueChange={(value) => patch({ dns_provider: value })}
              >
                <SelectTrigger id="cert-dns-provider-select">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {profiles.map((p) => (
                    <SelectItem key={p.id} value={String(p.id)}>
                      {p.name} — {p.provider_label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {selectedProfile && !selectedProfile.configured && (
                <p
                  role="alert"
                  className="text-sm text-amber-600 dark:text-amber-500"
                >
                  {t('certificates.wizard.providerNotConfigured', {
                    provider: selectedProfile.name,
                  })}
                </p>
              )}
            </div>
          )}
        </fieldset>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="cert-key-type">{t('certificates.wizard.keyType')}</Label>
          <Select
            value={form.key_type}
            onValueChange={(value) => patch({ key_type: value as AcmeKeyType })}
          >
            <SelectTrigger id="cert-key-type">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="ecdsa">{t('certificates.keyType.ecdsa')}</SelectItem>
              <SelectItem value="rsa">{t('certificates.keyType.rsa')}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-2">
          <Label htmlFor="cert-email">{t('certificates.wizard.email')}</Label>
          <Input
            id="cert-email"
            type="email"
            inputMode="email"
            autoComplete="off"
            placeholder={t('certificates.wizard.emailPlaceholder')}
            value={form.email}
            onChange={(event) => patch({ email: event.target.value })}
          />
          {errors.email !== undefined && (
            <p role="alert" className="text-sm text-destructive">
              {errors.email}
            </p>
          )}
        </div>
      </div>

      <div className="space-y-2 rounded-lg border p-3">
        <div className="flex items-center justify-between gap-4">
          <Label htmlFor="cert-staging" className="font-normal">
            {t('certificates.wizard.staging')}
          </Label>
          <Switch
            id="cert-staging"
            checked={form.staging}
            onCheckedChange={(checked) => patch({ staging: checked })}
          />
        </div>
        {form.staging && (
          <p className="text-xs text-destructive">
            {t('certificates.wizard.stagingNote')}
          </p>
        )}
      </div>

      {errors.form !== undefined && (
        <Alert variant="destructive">
          <AlertTitle>{t('certificates.wizard.createFailed')}</AlertTitle>
          <AlertDescription>{errors.form}</AlertDescription>
        </Alert>
      )}

      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onDone}
          disabled={saveMutation.isPending}
        >
          {t('common.cancel')}
        </Button>
        <Button type="submit" disabled={saveMutation.isPending}>
          {saveMutation.isPending && (
            <Loader2 className="animate-spin" aria-hidden="true" />
          )}
          {editing
            ? t('certificates.wizard.saveEdit')
            : t('certificates.wizard.submit')}
        </Button>
      </DialogFooter>
    </form>
  )
}

interface PrecheckPanelProps {
  query: UseMutationResult<CertPrecheck, Error, number>
  onRetry: () => void
  onDone: () => void
}

function PrecheckPanel({ query, onRetry, onDone }: PrecheckPanelProps) {
  const { t } = useTranslation()

  return (
    <div className="flex flex-col gap-4">
      <Alert variant="info">
        <Info aria-hidden="true" />
        <AlertTitle>{t('certificates.precheck.title')}</AlertTitle>
        <AlertDescription>{t('certificates.precheck.intro')}</AlertDescription>
      </Alert>

      {query.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
          {t('certificates.precheck.loading')}
        </div>
      ) : query.isError ? (
        <div className="flex flex-col items-start gap-3">
          <p className="text-sm text-destructive" role="alert">
            {query.error instanceof ApiError
              ? query.error.message
              : t('certificates.precheck.loadFailed')}
          </p>
          <Button variant="outline" size="sm" onClick={onRetry}>
            {t('common.retry')}
          </Button>
        </div>
      ) : query.data !== undefined ? (
        <div className="space-y-4">
          {query.data.resolvers.length > 0 && (
            <p className="text-sm text-muted-foreground">
              {t('certificates.precheck.resolvers')}:{' '}
              <span className="font-mono text-xs">
                {query.data.resolvers.join(', ')}
              </span>
            </p>
          )}
          {query.data.delegation_hints.map((hint) => (
            <div key={hint.domain} className="space-y-2 rounded-lg border p-3">
              <p className="font-mono text-xs font-medium">{hint.domain}</p>
              <p className="text-xs text-muted-foreground">
                {t('certificates.precheck.requires')}: {hint.requires}
              </p>
              <p className="text-xs font-medium">{t('certificates.precheck.records')}</p>
              <pre className="overflow-x-auto rounded-md border bg-muted/50 p-2 font-mono text-xs">
                {hint.records.join('\n')}
              </pre>
            </div>
          ))}
        </div>
      ) : null}

      <DialogFooter>
        <Button type="button" onClick={onDone}>
          {t('certificates.precheck.done')}
        </Button>
      </DialogFooter>
    </div>
  )
}

import { useQuery } from '@tanstack/react-query'
import { Plus, X } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
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
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/api'
import { isValidDomain } from '@/lib/domain'

interface ToggleRowProps {
  id: string
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
  disabled?: boolean
}

/** A label + Switch row, matching the proxy host editor. */
export function ToggleRow({
  id,
  label,
  checked,
  onChange,
  disabled,
}: ToggleRowProps) {
  return (
    <div className="flex items-center justify-between gap-4">
      <Label
        htmlFor={id}
        className="font-normal data-[disabled=true]:opacity-50"
        data-disabled={disabled}
      >
        {label}
      </Label>
      <Switch
        id={id}
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
      />
    </div>
  )
}

interface DomainChipsFieldProps {
  id: string
  domains: string[]
  onChange: (domains: string[]) => void
}

/**
 * The chip-style domain input shared by the redirection and 404 host editors.
 * Manages its own draft and inline validation error; the parent owns the list.
 */
export function DomainChipsField({ id, domains, onChange }: DomainChipsFieldProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState('')
  const [error, setError] = useState<string | null>(null)

  const add = () => {
    const candidate = draft.trim().toLowerCase()
    if (candidate === '') {
      return
    }
    if (!isValidDomain(candidate)) {
      setError(t('hosts.editor.invalidDomain'))
      return
    }
    if (domains.includes(candidate)) {
      setError(t('hosts.editor.duplicateDomain'))
      return
    }
    onChange([...domains, candidate])
    setDraft('')
    setError(null)
  }

  const remove = (domain: string) =>
    onChange(domains.filter((item) => item !== domain))

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{t('hosts.editor.domains')}</Label>
      <div className="flex flex-wrap gap-1.5">
        {domains.map((domain) => (
          <span
            key={domain}
            className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-sm"
          >
            {domain}
            <button
              type="button"
              onClick={() => remove(domain)}
              className="text-muted-foreground hover:text-foreground"
              aria-label={t('hosts.editor.removeDomain', { domain })}
            >
              <X className="size-3" aria-hidden="true" />
            </button>
          </span>
        ))}
      </div>
      <div className="flex gap-2">
        <Input
          id={id}
          value={draft}
          placeholder="example.com"
          onChange={(event) => {
            setDraft(event.target.value)
            setError(null)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ',') {
              event.preventDefault()
              add()
            }
          }}
        />
        <Button type="button" variant="outline" onClick={add}>
          <Plus aria-hidden="true" />
          {t('hosts.editor.addDomain')}
        </Button>
      </div>
      {error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}

export interface SslToggles {
  force_ssl: boolean
  http2: boolean
  hsts: boolean
  hsts_subdomains: boolean
}

interface HostSslFieldsProps {
  idPrefix: string
  certificateId: number | null
  onCertificateChange: (id: number | null) => void
  toggles: SslToggles
  onToggle: (patch: Partial<SslToggles>) => void
}

/**
 * The SSL tab shared by the redirection and 404 host editors: a certificate
 * picker plus the force-SSL / HTTP-2 / HSTS toggles. Reuses the proxy editor's
 * ['certificates'] query key and its wording.
 */
export function HostSslFields({
  idPrefix,
  certificateId,
  onCertificateChange,
  toggles,
  onToggle,
}: HostSslFieldsProps) {
  const { t } = useTranslation()

  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  const certificates = certsQuery.data?.certificates ?? []

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${idPrefix}-certificate`}>
          {t('hosts.editor.ssl.certificate')}
        </Label>
        <Select
          value={certificateId === null ? 'none' : String(certificateId)}
          onValueChange={(value) =>
            onCertificateChange(value === 'none' ? null : Number.parseInt(value, 10))
          }
        >
          <SelectTrigger id={`${idPrefix}-certificate`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">
              {t('hosts.editor.ssl.certificateNone')}
            </SelectItem>
            {certificates.map((cert) => (
              <SelectItem key={cert.id} value={String(cert.id)}>
                {cert.name} — {cert.domains.join(', ')}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {certsQuery.isError && (
          <p role="alert" className="text-sm text-destructive">
            {t('hosts.editor.ssl.loadFailed')}
          </p>
        )}
        <p className="text-sm text-muted-foreground">
          {certificateId === null
            ? t('hosts.editor.ssl.selectNote')
            : t('hosts.editor.ssl.activeNote')}
        </p>
      </div>
      <div className="space-y-3 rounded-lg border p-3">
        <ToggleRow
          id={`${idPrefix}-force-ssl`}
          label={t('hosts.editor.forceSsl')}
          checked={toggles.force_ssl}
          onChange={(checked) => onToggle({ force_ssl: checked })}
        />
        <ToggleRow
          id={`${idPrefix}-http2`}
          label={t('hosts.editor.http2')}
          checked={toggles.http2}
          onChange={(checked) => onToggle({ http2: checked })}
        />
        <ToggleRow
          id={`${idPrefix}-hsts`}
          label={t('hosts.editor.hsts')}
          checked={toggles.hsts}
          onChange={(checked) => onToggle({ hsts: checked })}
        />
        <ToggleRow
          id={`${idPrefix}-hsts-subdomains`}
          label={t('hosts.editor.hstsSubdomains')}
          checked={toggles.hsts_subdomains}
          onChange={(checked) => onToggle({ hsts_subdomains: checked })}
        />
      </div>
    </div>
  )
}

interface HostAdvancedFieldProps {
  id: string
  value: string
  onChange: (value: string) => void
}

/** The Advanced tab: the root-equivalent warning plus a snippet textarea. */
export function HostAdvancedField({ id, value, onChange }: HostAdvancedFieldProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>{t('hosts.editor.advanced.warningTitle')}</AlertTitle>
        <AlertDescription>{t('hosts.editor.advanced.warningBody')}</AlertDescription>
      </Alert>
      <div className="space-y-2">
        <Label htmlFor={id}>{t('hosts.editor.advanced.label')}</Label>
        <Textarea
          id={id}
          className="min-h-40 font-mono text-xs"
          value={value}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      </div>
    </div>
  )
}

/** A compact SSL badge for the host tables: emerald when a cert is attached. */
export function SslBadge({ hasCertificate }: { hasCertificate: boolean }) {
  const { t } = useTranslation()

  if (hasCertificate) {
    return (
      <Badge variant="success">
        {t('hosts.table.ssl')}
      </Badge>
    )
  }
  return <span className="text-muted-foreground">—</span>
}

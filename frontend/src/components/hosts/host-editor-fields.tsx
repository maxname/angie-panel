import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { ChipsField } from '@/components/chips-field'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
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
 * Domain rules on top of the shared chips input: lowercase what's typed, and
 * reject anything that isn't a domain or is already in the list.
 */
export function DomainChipsField({ id, domains, onChange }: DomainChipsFieldProps) {
  const { t } = useTranslation()

  return (
    <ChipsField
      id={id}
      label={t('hosts.editor.domains')}
      values={domains}
      onChange={onChange}
      placeholder="example.com"
      addLabel={t('hosts.editor.addDomain')}
      removeLabel={(domain) => t('hosts.editor.removeDomain', { domain })}
      normalize={(raw) => raw.trim().toLowerCase()}
      validate={(domain, existing) => {
        if (!isValidDomain(domain)) {
          return t('hosts.editor.invalidDomain')
        }
        if (existing.includes(domain)) {
          return t('hosts.editor.duplicateDomain')
        }
        return null
      }}
    />
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

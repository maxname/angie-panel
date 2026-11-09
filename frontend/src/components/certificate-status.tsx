import type { TFunction } from 'i18next'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import type { AcmeStatus } from '@/lib/api'

// Shared by the certificates page and the dashboard. It lives here rather than
// in either page so the dashboard doesn't drag the whole certificates page —
// dialogs, forms and all — into its chunk for one badge.

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
    return {
      kind: 'issued',
      label: t('certificates.status.issued'),
      hint: status.details,
    }
  }
  const raw = status.state ?? status.certificate
  if (raw !== undefined && raw !== '') {
    const lowered = raw.toLowerCase()
    const isError = ['expired', 'invalid', 'error', 'fail', 'revoked'].some(
      (word) => lowered.includes(word),
    )
    return {
      kind: isError ? 'error' : 'pending',
      label: raw.charAt(0).toUpperCase() + raw.slice(1),
      hint: status.details,
    }
  }
  return {
    kind: 'pending',
    label: t('certificates.status.pending'),
    hint: status.details,
  }
}

export function StatusPill({ status }: { status: AcmeStatus | null }) {
  const { t } = useTranslation()
  const derived = deriveStatus(status, t)

  if (derived.kind === 'issued') {
    return (
      <Badge variant="success" title={derived.hint}>
        {derived.label}
      </Badge>
    )
  }
  if (derived.kind === 'pending') {
    return (
      <Badge variant="warning" title={derived.hint}>
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
    <Badge
      variant="outline"
      className="text-muted-foreground"
      title={derived.hint}
    >
      {derived.label}
    </Badge>
  )
}

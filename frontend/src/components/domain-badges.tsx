import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'

/** True for `*.example.com` — a wildcard is a matching pattern, not a hostname
 *  you can browse to, so it never becomes a link. */
function isWildcard(domain: string): boolean {
  return domain.startsWith('*.')
}

/**
 * Read-only domain chips for the host/certificate lists. Every resolvable
 * domain opens the live site in a new tab; wildcards stay inert. `secure`
 * picks the scheme — pass it when the host actually serves HTTPS, otherwise
 * the link would land on a port that isn't listening.
 *
 * Not for the editors: chips there are removable drafts, not live sites.
 */
export function DomainBadges({
  domains,
  secure = false,
}: {
  domains: readonly string[]
  secure?: boolean
}) {
  const { t } = useTranslation()

  return (
    <div className="flex flex-wrap gap-1">
      {domains.map((domain) =>
        isWildcard(domain) ? (
          <Badge key={domain} variant="secondary" className="font-mono font-normal">
            {domain}
          </Badge>
        ) : (
          <a
            key={domain}
            href={`${secure ? 'https' : 'http'}://${domain}`}
            target="_blank"
            rel="noreferrer noopener"
            title={t('common.openInNewTab', { domain })}
            className="rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Badge
              variant="secondary"
              className="font-mono font-normal transition-colors hover:bg-primary/10 hover:text-primary"
            >
              {domain}
            </Badge>
          </a>
        ),
      )}
    </div>
  )
}

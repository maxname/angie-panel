import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { api, type HealthBeat } from '@/lib/api'
import { cn } from '@/lib/utils'

/** How many segments the bar draws. Fewer beats pad with blanks on the left, so
 *  the bar keeps its width and fills rightward as history accrues — newest at
 *  the right, Kuma-style. */
const SEGMENTS = 40

/**
 * A single combined uptime bar for a host, per the user's choice: the host is
 * up in a segment only if every check that reported there was up.
 *
 * The checks tick independently, so there are no shared time slots to intersect
 * — each probe is its own segment on one merged timeline, red if that probe
 * failed. With one check that is exactly the check; with two, any failure of
 * either shows red, which is what "green only if all green" means when you
 * cannot align them. The percentage is the success rate over what is shown.
 *
 * Only rendered for hosts that actually have an enabled check — the caller
 * decides that; an unprobed host shows nothing here rather than an empty bar.
 */
export function UptimeBar({ hostId }: { hostId: number }) {
  const { t } = useTranslation()
  const { data, isPending, isError } = useQuery({
    queryKey: ['hosts', hostId, 'health'],
    queryFn: () => api.hostHealth(hostId),
    // Live, but gently: the bar is glanceable, not a console.
    refetchInterval: 30_000,
    staleTime: 15_000,
  })

  if (isPending) {
    return <BarSkeleton />
  }
  if (isError) {
    return null
  }

  // Merge both kinds onto one timeline, oldest→newest, and keep the tail.
  const beats = [...data.beats]
    .sort((a, b) => a.ts - b.ts)
    .slice(-SEGMENTS)

  if (beats.length === 0) {
    // Configured but nothing recorded yet — the scheduler has not run a full
    // interval. Show the empty track and "—", not a lie about 100%.
    return (
      <div className="flex items-center gap-2">
        <PercentBadge label="—" tone="empty" />
        <Track>
          {Array.from({ length: SEGMENTS }, (_, i) => (
            <Segment key={i} tone="empty" />
          ))}
        </Track>
      </div>
    )
  }

  const up = beats.filter((b) => b.ok).length
  const percent = Math.round((up / beats.length) * 100)
  const pad = SEGMENTS - beats.length

  return (
    <div className="flex items-center gap-2">
      <PercentBadge
        label={`${percent}%`}
        tone={percent === 100 ? 'up' : percent >= 90 ? 'warn' : 'down'}
      />
      <Track>
        {Array.from({ length: pad }, (_, i) => (
          <Segment key={`pad-${i}`} tone="empty" />
        ))}
        {beats.map((beat, i) => (
          <Segment
            key={i}
            tone={beat.ok ? 'up' : 'down'}
            title={beatTitle(beat, t)}
          />
        ))}
      </Track>
    </div>
  )
}

function Track({ children }: { children: React.ReactNode }) {
  return <div className="flex items-center gap-[2px]">{children}</div>
}

function Segment({
  tone,
  title,
}: {
  tone: 'up' | 'down' | 'empty'
  title?: string
}) {
  return (
    <span
      title={title}
      className={cn(
        'h-4 w-[3px] rounded-[1px]',
        tone === 'up' && 'bg-success',
        tone === 'down' && 'bg-destructive',
        tone === 'empty' && 'bg-muted',
      )}
    />
  )
}

function PercentBadge({
  label,
  tone,
}: {
  label: string
  tone: 'up' | 'warn' | 'down' | 'empty'
}) {
  return (
    <span
      className={cn(
        'rounded-full px-1.5 py-0.5 text-xs font-medium tabular-nums',
        tone === 'up' && 'bg-success/10 text-success',
        tone === 'warn' && 'bg-warning/10 text-warning',
        tone === 'down' && 'bg-destructive/10 text-destructive',
        tone === 'empty' && 'bg-muted text-muted-foreground',
      )}
    >
      {label}
    </span>
  )
}

function BarSkeleton() {
  return (
    <div className="flex items-center gap-2">
      <span className="h-5 w-10 animate-pulse rounded-full bg-muted" />
      <div className="flex items-center gap-[2px]">
        {Array.from({ length: SEGMENTS }, (_, i) => (
          <span key={i} className="h-4 w-[3px] rounded-[1px] bg-muted" />
        ))}
      </div>
    </div>
  )
}

function beatTitle(beat: HealthBeat, t: (k: string) => string): string {
  const kind = beat.kind.toUpperCase()
  if (beat.ok) {
    const ms = beat.latency_ms != null ? ` · ${beat.latency_ms}ms` : ''
    return `${kind}: ${t('hosts.health.up')}${ms}`
  }
  return `${kind}: ${beat.error ?? t('hosts.health.down')}`
}

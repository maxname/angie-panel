import { useQuery } from '@tanstack/react-query'
import { Tooltip as TooltipPrimitive } from 'radix-ui'
import * as React from 'react'
import { useTranslation } from 'react-i18next'

import {
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
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
      {/* One shared provider for the whole bar rather than one per segment: a
          small open delay stops a quick sweep from flashing 40 tooltips, and
          skipDelay keeps them instant once you're already reading one. */}
      <TooltipProvider delayDuration={200} skipDelayDuration={400}>
        <Track>
          {Array.from({ length: pad }, (_, i) => (
            <Segment key={`pad-${i}`} tone="empty" />
          ))}
          {beats.map((beat, i) => (
            <TooltipPrimitive.Root key={i}>
              <TooltipTrigger asChild>
                <Segment tone={beat.ok ? 'up' : 'down'} interactive />
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                <BeatDetail beat={beat} t={t} />
              </TooltipContent>
            </TooltipPrimitive.Root>
          ))}
        </Track>
      </TooltipProvider>
    </div>
  )
}

function Track({ children }: { children: React.ReactNode }) {
  return <div className="flex items-center gap-[2px]">{children}</div>
}

const Segment = React.forwardRef<
  HTMLSpanElement,
  {
    tone: 'up' | 'down' | 'empty'
    interactive?: boolean
  } & React.HTMLAttributes<HTMLSpanElement>
>(function Segment({ tone, interactive, className, ...props }, ref) {
  return (
    <span
      ref={ref}
      className={cn(
        'h-4 w-[3px] rounded-[1px]',
        interactive && 'transition-transform hover:scale-y-125',
        tone === 'up' && 'bg-success',
        tone === 'down' && 'bg-destructive',
        tone === 'empty' && 'bg-muted',
        className,
      )}
      {...props}
    />
  )
})

/** Rich hover content for one probe: when, up or down, how fast, and — when it
 *  failed — why. This is what makes a red square worth hovering. */
function BeatDetail({
  beat,
  t,
}: {
  beat: HealthBeat
  t: (key: string, options?: Record<string, unknown>) => string
}) {
  const when = new Date(beat.ts * 1000).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
  return (
    <div className="space-y-0.5">
      <div className="flex items-center gap-1.5 font-medium">
        <span
          className={cn(
            'inline-block size-1.5 rounded-full',
            beat.ok ? 'bg-success' : 'bg-destructive',
          )}
        />
        {beat.kind.toUpperCase()} · {beat.ok ? t('hosts.health.up') : t('hosts.health.down')}
      </div>
      <div className="text-primary-foreground/70">{when}</div>
      {beat.ok && beat.latency_ms != null && (
        <div className="text-primary-foreground/70">
          {t('hosts.health.latency', { ms: beat.latency_ms })}
        </div>
      )}
      {!beat.ok && beat.error && (
        <div className="break-words text-primary-foreground/70">{beat.error}</div>
      )}
    </div>
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

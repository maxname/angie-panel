import { type ReactNode } from 'react'

import { cn } from '@/lib/utils'

export type RouteState = 'live' | 'pending' | 'off'

/**
 * Renders a routing row as a physical patch connection: `left ──● right`.
 * The jack (LED) carries the live/pending/off state visually; callers pair it
 * with a text status label elsewhere in the row so state never rides on colour
 * alone.
 */
export function RouteCable({
  left,
  right,
  state = 'live',
}: {
  left: ReactNode
  right: ReactNode
  state?: RouteState
}) {
  return (
    <div className="flex min-w-0 items-center gap-3">
      <div className="flex flex-wrap items-center gap-1">{left}</div>
      <span
        className={cn('route-wire', state === 'off' && 'is-off')}
        aria-hidden="true"
      />
      <span
        className={cn(
          'route-jack',
          state === 'live' && 'is-live',
          state === 'pending' && 'is-pending',
          state === 'off' && 'is-off',
        )}
        aria-hidden="true"
      />
      <span className="shrink-0 font-mono text-xs whitespace-nowrap text-muted-foreground">
        {right}
      </span>
    </div>
  )
}

import { CornerUpRight, Unplug } from 'lucide-react'
import { RadioGroup } from 'radix-ui'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { DefaultSite } from '@/lib/api'

const OPTIONS: DefaultSite[] = ['notfound', 'drop444', 'redirect', 'html']

/**
 * The four answers Angie can give an unrecognised domain, each drawn as what
 * the visitor would see. A dropdown made you read four sentences to compare
 * them; the sketches are the difference at a glance, and the sentence stays
 * underneath for the detail.
 *
 * The sketches are markup rather than images: they're built from the same
 * tokens as the rest of the UI, so they follow the theme for free and can't
 * drift into a second source of colour.
 */
function Sketch({ kind }: { kind: DefaultSite }) {
  return (
    <div className="pointer-events-none flex h-16 flex-col overflow-hidden rounded-md border bg-background">
      {/* Browser chrome, so the panel reads as "what the visitor gets". */}
      <div className="flex h-3 shrink-0 items-center gap-[3px] border-b bg-muted px-1.5">
        <span className="size-1 rounded-full bg-muted-foreground/40" />
        <span className="size-1 rounded-full bg-muted-foreground/40" />
        <span className="size-1 rounded-full bg-muted-foreground/40" />
      </div>
      <div className="flex flex-1 items-center justify-center px-3">
        {kind === 'notfound' && (
          <span className="font-mono text-xs font-medium text-muted-foreground">
            404
          </span>
        )}
        {/* Nothing comes back at all — an empty frame says that better than
            any glyph could, so the icon just labels the emptiness. */}
        {kind === 'drop444' && (
          <Unplug className="size-3.5 text-muted-foreground/70" />
        )}
        {kind === 'redirect' && (
          <CornerUpRight className="size-3.5 text-muted-foreground/70" />
        )}
        {kind === 'html' && (
          <div className="w-full space-y-1">
            <div className="h-1 w-1/2 rounded-full bg-muted-foreground/40" />
            <div className="h-1 w-full rounded-full bg-muted-foreground/25" />
            <div className="h-1 w-3/4 rounded-full bg-muted-foreground/25" />
          </div>
        )}
      </div>
    </div>
  )
}

interface DefaultSitePickerProps {
  value: string
  onChange: (value: DefaultSite) => void
  'aria-labelledby'?: string
}

export function DefaultSitePicker({
  value,
  onChange,
  'aria-labelledby': labelledBy,
}: DefaultSitePickerProps) {
  const { t } = useTranslation()

  return (
    <RadioGroup.Root
      value={value}
      onValueChange={(next) => onChange(next as DefaultSite)}
      aria-labelledby={labelledBy}
      className="grid grid-cols-2 gap-3 lg:grid-cols-4"
    >
      {OPTIONS.map((option) => (
        <RadioGroup.Item
          key={option}
          value={option}
          className={cn(
            'group flex cursor-pointer flex-col gap-2 rounded-lg border bg-card p-2 text-left transition-colors',
            'hover:border-primary/40 focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none',
            'data-[state=checked]:border-primary data-[state=checked]:ring-[3px] data-[state=checked]:ring-primary/20',
          )}
        >
          <Sketch kind={option} />
          <span className="px-1 pb-0.5 text-xs text-muted-foreground group-data-[state=checked]:font-medium group-data-[state=checked]:text-foreground">
            {t(`settings.defaultSite.options.${option}`)}
          </span>
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  )
}

import { cn } from '@/lib/utils'

/**
 * The Angie "A", in the accent's hue.
 *
 * Two files rather than one, because one cannot work. At hue 293° there is no
 * lightness that scores 4.5 against both the light sidebar (#F9F4F1) and the
 * dark one (#1B1613) — the requirements exclude each other. Aiming between them
 * is what makes the mark look disabled next to the button. This is the same
 * reason --primary is a pair (#6D28D9 / #7C3AED); a raster mark just has to
 * carry its pair as files. The light one sits at the button's weight (L .50,
 * C .22 against its .49 / .24); the dark one is lifted to L .60 so it reads on
 * charcoal, which also lands it brighter than the dark button — the right side
 * of wrong for a brand mark.
 *
 * Both are in the markup, and CSS picks: switching themes fetches nothing and
 * cannot flash. alt="" on both — decorative, since the product name is always
 * next to it as text.
 *
 * Regenerate either from the source artwork with scratchpad/extract-a.swift:
 * hue, then median lightness and chroma, are its 4th–6th arguments.
 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <>
      <img src="/logo.png" alt="" className={cn('dark:hidden', className)} />
      <img
        src="/logo-dark.png"
        alt=""
        className={cn('hidden dark:block', className)}
      />
    </>
  )
}

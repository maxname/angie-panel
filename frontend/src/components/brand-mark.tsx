import { cn } from '@/lib/utils'

/**
 * The Angie "A", in the accent's hue (293°) with the gradient carried by
 * lightness — purple to dark purple, at the light button's weight.
 *
 * One file for both themes, by choice, and it favours the light one. Measured
 * against the sidebar it sits on: 6.5 contrast on #F9F4F1, but 2.60 on
 * #1B1613 (3.30 at its lightest, 1.73 at its darkest) — under the 3.0 a
 * graphic wants. No single weight clears both; that is why --primary is a pair
 * (#6D28D9 / #7C3AED). So this is a choice of theme, not a compromise serving
 * both, and the dark sidebar is where it is paid for.
 *
 * To give dark its own again: regenerate with a lighter median (L .60 reads at
 * ~5 there and keeps chroma clipping to 6%; L .66 looks obvious but purple's
 * gamut runs out and flattens 43% of the pixels), then render two <img> and
 * swap them on `dark:` — both sit in the markup, so CSS picks and nothing is
 * fetched on a theme change. scratchpad/extract-a.swift takes hue, median
 * lightness, median chroma and hue-spread as its 4th–7th arguments.
 *
 * alt="" — decorative: the product name is always next to it as text.
 */
export function BrandMark({ className }: { className?: string }) {
  return <img src="/logo.png" alt="" className={cn(className)} />
}

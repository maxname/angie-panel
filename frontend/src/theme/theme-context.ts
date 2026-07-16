import { createContext, useContext } from 'react'

/** What the operator picked. 'system' defers to the OS, and keeps deferring. */
export type Theme = 'light' | 'dark' | 'system'

/** What is actually painted — 'system' resolved against the OS preference. */
export type ResolvedTheme = 'light' | 'dark'

export const THEMES: readonly Theme[] = ['light', 'dark', 'system']

export interface ThemeContextValue {
  /** The stored preference — what the picker shows a tick against. */
  theme: Theme
  /** The painted theme. Use this to decide what a control should look like. */
  resolvedTheme: ResolvedTheme
  setTheme: (theme: Theme) => void
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (context === null) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }
  return context
}

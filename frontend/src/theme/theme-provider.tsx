import { useEffect, useMemo, useState, type ReactNode } from 'react'

import { ThemeContext, type ResolvedTheme, type Theme } from './theme-context'

const THEME_STORAGE_KEY = 'angie-panel-theme'
const DARK_QUERY = '(prefers-color-scheme: dark)'

// localStorage throws in sandboxed iframes and "block all cookies" modes. This
// runs during ThemeProvider's render (which wraps the whole app), so an
// unguarded throw would blank the entire UI — fall back to following the OS.
function readStoredTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY)
    if (stored === 'light' || stored === 'dark' || stored === 'system') {
      return stored
    }
  } catch {
    // Storage unavailable — fall through to the default.
  }
  // Follow the OS until told otherwise. The old code read the OS preference
  // once and then stored *that*, which quietly froze the theme for anyone who
  // never touched the toggle: your Mac switching to dark at sunset left the
  // panel light forever.
  return 'system'
}

function systemTheme(): ResolvedTheme {
  return window.matchMedia(DARK_QUERY).matches ? 'dark' : 'light'
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(readStoredTheme)
  const [systemPreference, setSystemPreference] =
    useState<ResolvedTheme>(systemTheme)

  // Keep following the OS for as long as 'system' is picked — the point of the
  // option is that it keeps up, not that it samples once at boot.
  useEffect(() => {
    const query = window.matchMedia(DARK_QUERY)
    const onChange = (event: MediaQueryListEvent) => {
      setSystemPreference(event.matches ? 'dark' : 'light')
    }
    query.addEventListener('change', onChange)
    return () => query.removeEventListener('change', onChange)
  }, [])

  const resolvedTheme = theme === 'system' ? systemPreference : theme

  useEffect(() => {
    document.documentElement.classList.toggle('dark', resolvedTheme === 'dark')
    // Keep the browser chrome (mobile address bar) matching the page background.
    const meta = document.querySelector('meta[name="theme-color"]')
    if (meta) {
      meta.setAttribute(
        'content',
        resolvedTheme === 'dark' ? '#0E141E' : '#F4F6F8',
      )
    }
  }, [resolvedTheme])

  useEffect(() => {
    try {
      localStorage.setItem(THEME_STORAGE_KEY, theme)
    } catch {
      // Persisting the preference is best-effort; ignore storage failures.
    }
  }, [theme])

  const value = useMemo(
    () => ({ theme, resolvedTheme, setTheme }),
    [theme, resolvedTheme],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

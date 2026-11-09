import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

export const LANGUAGE_STORAGE_KEY = 'angie-panel-lang'

export const SUPPORTED_LANGUAGES = ['en', 'ru'] as const
export type Language = (typeof SUPPORTED_LANGUAGES)[number]

/**
 * Each language named in itself — a menu of languages is the one place you
 * cannot translate the labels, since someone who lands in the wrong language
 * has to find their way out.
 *
 * Adding a language means: a catalogue file, an entry here, and a loader in
 * `catalogues` below. Nothing else in the UI enumerates them.
 */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: 'English',
  ru: 'Русский',
}

function isSupported(value: string | null | undefined): value is Language {
  return !!value && (SUPPORTED_LANGUAGES as readonly string[]).includes(value)
}

/**
 * Resolve the language up front, to one we definitely have: a stored choice
 * wins, then the browser's preference by base tag (`ru-RU` → `ru`), English
 * otherwise.
 *
 * Doing this ourselves rather than letting i18next resolve it during init is
 * what lets us ship one catalogue instead of two: i18next eagerly loads
 * `fallbackLng` next to the active language, so keeping a fallback would put
 * every Russian user through the English catalogue as well, for nothing.
 */
export function pickLanguage(
  stored: string | null,
  preferred: readonly string[],
): Language {
  if (isSupported(stored)) return stored
  for (const tag of preferred) {
    if (isSupported(tag.split('-')[0])) return tag.split('-')[0] as Language
  }
  return 'en'
}

const catalogues: Record<Language, () => Promise<Record<string, unknown>>> = {
  en: () => import('./en').then((m) => m.en),
  ru: () => import('./ru').then((m) => m.ru),
}

function storedLanguage(): string | null {
  try {
    return localStorage.getItem(LANGUAGE_STORAGE_KEY)
  } catch {
    return null // Private mode / storage disabled: fall through to the browser.
  }
}

const initial = pickLanguage(
  storedLanguage(),
  navigator.languages ?? [navigator.language],
)

// Top-level await: the module graph settles before main.tsx renders, so the
// first paint already has its strings — no Suspense boundary, no flash of raw
// keys.
await i18n.use(initReactI18next).init({
  lng: initial,
  resources: { [initial]: { translation: await catalogues[initial]() } },
  supportedLngs: SUPPORTED_LANGUAGES,
  // Every catalogue carries the full key set — parity.test.ts fails the build
  // otherwise — so there is nothing to fall back to.
  fallbackLng: false,
  interpolation: {
    // React already escapes rendered strings.
    escapeValue: false,
  },
})

/**
 * Switch language, fetching the catalogue first: it is not in the bundle until
 * someone asks for it. Use this rather than `i18n.changeLanguage`, which would
 * switch to a language whose strings have not arrived.
 */
export async function changeLanguage(next: Language): Promise<void> {
  if (!i18n.hasResourceBundle(next, 'translation')) {
    i18n.addResourceBundle(next, 'translation', await catalogues[next]())
  }
  await i18n.changeLanguage(next)
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, next)
  } catch {
    // Storage disabled — the choice just won't outlive the tab.
  }
}

i18n.on('languageChanged', (lng) => {
  document.documentElement.lang = lng
})
document.documentElement.lang = i18n.language

export default i18n

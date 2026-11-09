import { describe, expect, it } from 'vitest'

import { en } from './en'
import { ru } from './ru'
import { pickLanguage } from './index'

type Tree = { [key: string]: string | Tree }

function keys(tree: Tree, prefix = ''): string[] {
  return Object.entries(tree).flatMap(([k, v]) =>
    typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  )
}

// The panel loads exactly one catalogue and has no fallback language, so a key
// present in one locale and missing in the other renders as a raw `some.key`
// to those users. That trade is only safe while this holds.
describe('translation catalogues', () => {
  it('carry the same keys in both locales', () => {
    const inEn = new Set(keys(en as Tree))
    const inRu = new Set(keys(ru as Tree))
    expect([...inEn].filter((k) => !inRu.has(k))).toEqual([])
    expect([...inRu].filter((k) => !inEn.has(k))).toEqual([])
  })

  it('leave no value empty', () => {
    const empty = (tree: Tree, locale: string) =>
      keys(tree).filter((k) => {
        const value = k
          .split('.')
          .reduce<string | Tree | undefined>(
            (node, part) =>
              typeof node === 'object' ? node[part] : undefined,
            tree,
          )
        return typeof value === 'string' && value.trim() === ''
      }).map((k) => `${locale}:${k}`)
    expect([...empty(en as Tree, 'en'), ...empty(ru as Tree, 'ru')]).toEqual([])
  })
})

describe('pickLanguage', () => {
  it('honours a stored choice over the browser', () => {
    expect(pickLanguage('ru', ['en-US'])).toBe('ru')
    expect(pickLanguage('en', ['ru-RU'])).toBe('en')
  })

  it('falls through to the browser when nothing is stored', () => {
    expect(pickLanguage(null, ['ru-RU', 'en-US'])).toBe('ru')
    expect(pickLanguage(null, ['en-GB'])).toBe('en')
  })

  it('ignores a stored language we no longer ship', () => {
    expect(pickLanguage('de', ['ru'])).toBe('ru')
  })

  // The case that made a fallback language look necessary: without a match we
  // must still land on a language we have, not on raw keys.
  it('defaults to English for an unsupported browser', () => {
    expect(pickLanguage(null, ['fr-FR', 'de-DE'])).toBe('en')
    expect(pickLanguage(null, [])).toBe('en')
  })
})

// Vite inlines every source file at transform time, so this needs no
// filesystem access — src/ is browser code, and its tsconfig deliberately
// carries no node types.
const SOURCES = import.meta.glob('../**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

describe('t() call sites', () => {
  it('reference keys that exist', () => {
    const known = new Set(keys(en as Tree))
    const missing: string[] = []
    const scanned: string[] = []
    for (const [file, src] of Object.entries(SOURCES)) {
      if (/\.test\.tsx?$/.test(file)) continue
      scanned.push(file)
      for (const m of src.matchAll(/\bt\(\s*'([^']+)'\s*[,)]/g)) {
        if (!known.has(m[1])) missing.push(`${file}: t('${m[1]}')`)
      }
    }
    // Guard the guard: a glob that matched nothing would pass silently.
    expect(scanned.length).toBeGreaterThan(20)
    expect(missing).toEqual([])
  })
})

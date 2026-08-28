import { afterEach, beforeEach, describe, expect, it } from 'vitest'

// Renamed imports: `it` would collide with the `it` test function imported
// from vitest above (same identifier in the same module scope — a parse
// error, not a module resolution one).
import enMessages from './en.json'
import { applyProfileLocale, i18n, setLocale } from './index'
import itMessages from './it.json'

/// Flattens a nested object into a list of dotted keys.
function keys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === 'object' && v !== null
      ? keys(v as Record<string, unknown>, `${prefix}${k}.`)
      : [`${prefix}${k}`]
  )
}

describe('translations', () => {
  it('Italian and English have the same keys', () => {
    const itKeys = keys(itMessages).sort()
    const enKeys = keys(enMessages).sort()
    expect(itKeys).toEqual(enKeys)
  })

  it('no translation is empty', () => {
    for (const [locale, messages] of [['it', itMessages], ['en', enMessages]] as const) {
      for (const key of keys(messages)) {
        const value = key.split('.').reduce<unknown>(
          (acc, part) => (acc as Record<string, unknown>)[part],
          messages
        )
        expect(value, `${locale}.${key}`).not.toBe('')
      }
    }
  })
})

describe('users.locale as the source of truth', () => {
  beforeEach(() => {
    localStorage.clear()
    setLocale('en')
  })

  afterEach(() => {
    localStorage.clear()
    setLocale('en')
  })

  it('applyProfileLocale syncs i18n and localStorage from the profile', () => {
    applyProfileLocale('it')
    expect(i18n.global.locale.value).toBe('it')
    expect(localStorage.getItem('keeppix.locale')).toBe('it')
    expect(document.documentElement.lang).toBe('it')
  })

  it('applyProfileLocale ignores a missing or unsupported locale', () => {
    setLocale('en')
    applyProfileLocale(null)
    expect(i18n.global.locale.value).toBe('en')
    applyProfileLocale('fr')
    expect(i18n.global.locale.value).toBe('en')
  })
})

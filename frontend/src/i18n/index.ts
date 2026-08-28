import { createI18n } from 'vue-i18n'

import en from './en.json'
import it from './it.json'

const SUPPORTED = ['it', 'en'] as const
export type Locale = (typeof SUPPORTED)[number]

const STORAGE_KEY = 'keeppix.locale'

/** No hardcoded default language: it's detected, but an explicit choice
 * always wins. */
export function detectLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored && SUPPORTED.includes(stored as Locale)) {
    return stored as Locale
  }
  const preferred = navigator.languages ?? [navigator.language]
  for (const tag of preferred) {
    const base = tag.split('-')[0]
    if (SUPPORTED.includes(base as Locale)) {
      return base as Locale
    }
  }
  return 'en'
}

export function setLocale(locale: Locale): void {
  localStorage.setItem(STORAGE_KEY, locale)
  i18n.global.locale.value = locale
  document.documentElement.lang = locale
}

/**
 * The user profile is the source of truth once a session exists.
 * `localStorage` remains a first-paint / logged-out cache, synced from here.
 */
export function applyProfileLocale(locale: string | null | undefined): void {
  if (locale && SUPPORTED.includes(locale as Locale)) {
    setLocale(locale as Locale)
  }
}

/**
 * Plurals: this uses **vue-i18n's native syntax** (`'one photo | {n} photos'`),
 * not ICU MessageFormat.
 *
 * The goal is "correct pluralization". For Italian and English, the two
 * syntaxes give the same result: both languages have two plural categories
 * (CLDR `one`/`other`), which is exactly what the native form expresses.
 * ICU would cost an extra runtime dependency (`intl-messageformat`, ~25 KB
 * gzip against a 150 KB budget already mostly used) plus a custom
 * `messageCompiler` to maintain, for zero observable difference on the
 * languages currently shipped.
 *
 * **When to revisit this:** when adding the first language with more than
 * two plural categories — Russian, Polish, Arabic. At that point the right
 * choice is a build-time ICU compiler (`@intlify/unplugin-vue-i18n`), not a
 * runtime one; and by then the plural keys that would need rewriting are
 * countable, whereas today there are zero. `@intlify/core-base` was
 * previously listed in `dependencies` and never imported anywhere — a
 * leftover from an abandoned attempt — and has since been removed.
 */
export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: 'en',
  messages: { it, en }
})

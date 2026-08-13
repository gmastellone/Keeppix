import { createI18n } from 'vue-i18n'

import en from './en.json'
import it from './it.json'

const SUPPORTED = ['it', 'en'] as const
export type Locale = (typeof SUPPORTED)[number]

const STORAGE_KEY = 'keeppix.locale'

/** Nessuna lingua predefinita: si rileva, poi vince la scelta esplicita. */
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

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: 'en',
  messages: { it, en }
})

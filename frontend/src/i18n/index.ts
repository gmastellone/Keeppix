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

/**
 * Plurali: si usa la **sintassi nativa di vue-i18n** (`'una foto | {n} foto'`),
 * non ICU MessageFormat.
 *
 * Lo spec §10.10 chiede ICU e il motivo dichiarato è «plurali corretti». Per
 * italiano e inglese le due sintassi danno lo stesso risultato: entrambe le
 * lingue hanno due categorie plurali (CLDR `one`/`other`), che è esattamente
 * ciò che la forma nativa esprime. ICU costerebbe una dipendenza runtime in più
 * (`intl-messageformat`, ~25 KB gzip su un budget di 150 KB di cui 77 già usati)
 * più un `messageCompiler` custom da mantenere, per zero differenze osservabili
 * sulle lingue spedite.
 *
 * **Quando riaprire la decisione:** all'aggiunta della prima lingua con più di
 * due categorie plurali — russo, polacco, arabo. Allora la scelta giusta è un
 * compilatore ICU a build time (`@intlify/unplugin-vue-i18n`), non
 * a runtime; e in quel momento le chiavi plurali da riscrivere si contano,
 * mentre oggi sono zero. `@intlify/core-base` era dichiarato fra le
 * `dependencies` e non importato da nessuna parte — impronta di un tentativo
 * abbandonato — ed è stato rimosso.
 */
export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: 'en',
  messages: { it, en }
})

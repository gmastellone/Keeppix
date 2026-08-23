export type ScrubberBucket = {
  month: string
  count: number
}

/**
 * I mesi sono equidistanti sulla barra, **non** proporzionali al numero di
 * foto (documento funzionale §8.3, testuale: "i mesi sono equidistanti
 * sulla barra anche se uno contiene 5 foto e un altro 300") —
 * `Math.round(ratio*(offsets.length-1))` del prototipo (mockup riga 4870),
 * non un peso per `count`. `y`/`trackHeight` sono già al netto di
 * qualunque padding del binario: la responsabilità del chiamante.
 */
export function monthAtOffset(
  buckets: ScrubberBucket[],
  y: number,
  trackHeight: number
): string | undefined {
  if (buckets.length === 0 || trackHeight <= 0) return undefined
  const ratio = Math.min(1, Math.max(0, y / trackHeight))
  const index = Math.round(ratio * (buckets.length - 1))
  return buckets[index]?.month
}

/** `"YYYY-MM"` → `Date` UTC al primo del mese, per `Intl.DateTimeFormat`. */
function monthDate(month: string): Date {
  const [year, mm] = month.split('-').map(Number)
  return new Date(Date.UTC(year, mm - 1, 1))
}

/**
 * Etichetta abbreviata di una tick dello scrubber ("Lug", "Jul" secondo la
 * lingua) — `Intl.DateTimeFormat`, non una tabella di stringhe italiane
 * scritta a mano come nel prototipo (`MONTHS`, mockup riga 1523): l'app
 * supporta IT/EN, un nome di mese è testo localizzato come ogni altra
 * stringa dell'interfaccia, non un valore fisso.
 */
export function monthAbbrev(month: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { month: 'short', timeZone: 'UTC' }).format(monthDate(month))
}

/** Etichetta estesa con anno ("Luglio 2026", "July 2026") — il tooltip
 * dello scrubber durante il trascinamento (§8.3). */
export function monthFull(month: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(
    monthDate(month)
  )
}

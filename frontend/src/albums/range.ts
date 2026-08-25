import { monthFull } from '@/timeline/scrubber'

/** §41/§42, l'`<intervallo>` di `"<N> foto · <intervallo>"`: il mockup lo
 * legge da una stringa statica `a.range` scritta a mano nei dati di
 * partenza (`ALBUMS`) — sul backend reale non esiste un campo simile,
 * va calcolato dalle date scatto dei membri effettivi (`taken_at_utc`),
 * stesso principio già preso per `monthFull`/`monthAbbrev` dello
 * scrubber: mesi localizzati via `Intl.DateTimeFormat`, non una tabella
 * di stringhe italiane. `null` quando l'album non ha (ancora) membri con
 * una data nota — il chiamante sceglie il testo di ripiego appropriato
 * (album manuale vuoto vs. filtro dinamico senza corrispondenze). */
export function albumMonthRange(assets: { taken_at_utc: string | null }[], locale: string): string | null {
  const months = assets
    .map((asset) => asset.taken_at_utc)
    .filter((value): value is string => Boolean(value))
    .map((value) => value.slice(0, 7))
  if (months.length === 0) return null
  months.sort()
  const first = months[0]!
  const last = months[months.length - 1]!
  return first === last ? monthFull(first, locale) : `${monthFull(first, locale)} – ${monthFull(last, locale)}`
}

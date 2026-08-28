import { monthFull } from '@/timeline/scrubber'

/** The `<range>` in `"<N> photos · <range>"`: the mockup reads it from a
 * static `a.range` string hand-written into its seed data (`ALBUMS`) —
 * on the real backend there's no such field, it has to be computed from
 * the actual members' shot dates (`taken_at_utc`), the same principle
 * already used for the scrubber's `monthFull`/`monthAbbrev`: months
 * localized via `Intl.DateTimeFormat`, not a table of Italian strings.
 * `null` when the album has no (yet) members with a known date — the
 * caller picks the appropriate fallback text (empty manual album vs. a
 * dynamic filter with no matches). */
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

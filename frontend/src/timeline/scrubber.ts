export type ScrubberBucket = {
  month: string
  count: number
}

/**
 * Months are evenly spaced on the bar, **not** proportional to photo
 * count — the scrubber positions months equally even if one has 5
 * photos and another has 300. `Math.round(ratio*(offsets.length-1))`
 * from the prototype, not a weight based on `count`. `y`/`trackHeight`
 * are already net of any track padding: that's the caller's
 * responsibility.
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

/** `"YYYY-MM"` → UTC `Date` on the first of the month, for `Intl.DateTimeFormat`. */
function monthDate(month: string): Date {
  const [year, mm] = month.split('-').map(Number)
  return new Date(Date.UTC(year, mm - 1, 1))
}

/**
 * Abbreviated label for a scrubber tick ("Jul" depending on the
 * language) — `Intl.DateTimeFormat`, not a hand-written table of
 * Italian strings like the prototype's `MONTHS`: the app supports
 * IT/EN, a month name is localized text like any other UI string, not
 * a fixed value.
 */
export function monthAbbrev(month: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { month: 'short', timeZone: 'UTC' }).format(monthDate(month))
}

/** Full label with year ("July 2026") — the scrubber's tooltip while
 * dragging. */
export function monthFull(month: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(
    monthDate(month)
  )
}

/** "2026" — the scrubber's resting label. Every month still gets its own
 * tick (unlabeled) for drag precision; only the first bucket of each
 * year is labeled with text, so a 15-year library reads as a spaced-out
 * year axis instead of ~180 crowded month labels. */
export function yearLabel(month: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { year: 'numeric', timeZone: 'UTC' }).format(monthDate(month))
}

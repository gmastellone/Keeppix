/**
 * Construction of derivative URLs.
 *
 * The `/media/...` routes respond with `Cache-Control: … immutable`: the
 * browser won't revalidate for a year. But the hash in the URL addresses
 * the **source** file, not the bytes actually served, and those depend on
 * how we produce them. Changing the recipe — format, quality, dimensions,
 * embedded vs. demosaiced — makes the same URL return a different image,
 * and whoever already has the old one cached keeps it forever.
 *
 * Appending the recipe version makes the URL a true content key: a new
 * recipe produces new URLs, and the cache invalidates itself without
 * giving up `immutable` (which, on the timeline, saves hundreds of
 * revalidation requests).
 *
 * The value must stay in sync with `DERIVATIVE_VERSION` in
 * `crates/keeppix-media/src/derive.rs`: a test in `keeppix-api` checks
 * this, so changing just one of them breaks the build.
 */
export const DERIVATIVE_VERSION = 2

/** Invalidation suffix, the same for every derivative. */
function v(): string {
  return `?v=${DERIVATIVE_VERSION}`
}

// The paths are written **out in full** in each of the three functions
// instead of being composed from a parameter (`/media/${kind}/…`). This
// isn't careless duplication: `scripts/check-wired.py` verifies that
// every mounted route has a frontend consumer by searching for the
// literal string. Composing them dynamically would make the routes
// invisible to that check, which would then flag them as unused — and
// that actually happened once, the first time this file was written.

/** 240px thumbnail: timeline grid, search, filmstrip. */
export function thumbSrc(hash: string): string {
  return `/media/thumb/${hash}${v()}`
}

/** 2048px preview: opening a photo, culling, comparison. */
export function previewSrc(hash: string): string {
  return `/media/preview/${hash}${v()}`
}

/**
 * High-resolution rendition for culling zoom. Generated **lazily** on
 * first request: for RAW files this can require a demosaic, so seconds
 * rather than milliseconds.
 */
export function fullSrc(hash: string): string {
  return `/media/full/${hash}${v()}`
}

/**
 * "Download original": the real bytes of the source file,
 * `GET /media/original/{id}` (`routes/media.rs`). Keyed by **asset id**,
 * not by hash like the derivatives above: the original has no recipe to
 * invalidate, no `?v=`.
 */
export function originalSrc(assetId: string): string {
  return `/media/original/${assetId}`
}

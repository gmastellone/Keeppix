// An album cover is a generated CSS gradient, not a real thumbnail —
// `AlbumView.cover_tint`/`monochrome` exist as columns but no route ever
// writes them (`PatchAlbumBody` has no such fields), so they're always
// absent/`false` in practice. Computed client-side instead, deterministic
// on the id — the same approach used for `avatarColorFor`: hash-based,
// with no exact formula beyond that constraint.
export function albumCoverGradient(seed: string): string {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  }
  const hue = hash % 360
  return `linear-gradient(135deg, hsl(${hue}, 40%, 55%), hsl(${(hue + 18) % 360}, 35%, 32%))`
}

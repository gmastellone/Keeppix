// The current user's avatar color is a personal choice (a fixed set of
// swatches, picked in Profile); the color of **other** users in a share is
// hash-based via hsl(), independent of that personal choice — no exact
// formula, just the hash→hsl constraint. Deterministic on the same id/name:
// the same person always gets the same color, wherever they appear (People
// view, "Share selection").
export function avatarColorFor(seed: string): string {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  }
  const hue = hash % 360
  // Fixed saturation/lightness, chosen to stay legible with white text on
  // top across the whole range of hues.
  return `hsl(${hue}, 55%, 40%)`
}

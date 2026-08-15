export type ScrubberBucket = {
  month: string
  count: number
}

/** I bucket arrivano dal più recente. `y` è 0 in cima alla traccia. */
export function monthAtOffset(
  buckets: ScrubberBucket[],
  y: number,
  trackHeight: number
): string | undefined {
  if (buckets.length === 0 || trackHeight <= 0) return undefined
  const total = buckets.reduce((sum, b) => sum + Math.max(b.count, 1), 0)
  const t = Math.min(1, Math.max(0, y / trackHeight))
  let acc = 0
  for (const bucket of buckets) {
    acc += Math.max(bucket.count, 1) / total
    if (t <= acc) return bucket.month
  }
  return buckets.at(-1)?.month
}

export function yearLabel(month: string): string {
  return month.slice(0, 4)
}

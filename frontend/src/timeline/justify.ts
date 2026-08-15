export type AspectItem = {
  id: string
  width: number
  height: number
}

export type JustifiedCell = {
  id: string
  x: number
  y: number
  w: number
  h: number
}

export type JustifiedRow = {
  y: number
  height: number
  cells: JustifiedCell[]
}

/** Righe di altezza costante, larghezze proporzionali all'aspect ratio. */
export function justify(
  items: AspectItem[],
  containerWidth: number,
  targetRowHeight: number
): JustifiedRow[] {
  if (containerWidth <= 0 || targetRowHeight <= 0 || items.length === 0) {
    return []
  }
  const gap = 4
  const rows: JustifiedRow[] = []
  let row: AspectItem[] = []
  let rowWidth = 0
  let y = 0

  const flush = (last: boolean) => {
    if (row.length === 0) return
    const ratios = row.map((item) => item.width / Math.max(item.height, 1))
    const ratioSum = ratios.reduce((a, b) => a + b, 0)
    const gaps = gap * (row.length - 1)
    let height = last && rowWidth < containerWidth
      ? targetRowHeight
      : (containerWidth - gaps) / ratioSum
    height = Math.max(8, height)
    let x = 0
    const cells: JustifiedCell[] = row.map((item, i) => {
      const w = ratios[i] * height
      const cell = { id: item.id, x, y, w, h: height }
      x += w + gap
      return cell
    })
    rows.push({ y, height, cells })
    y += height + gap
    row = []
    rowWidth = 0
  }

  for (const item of items) {
    const w = (item.width / Math.max(item.height, 1)) * targetRowHeight
    if (row.length > 0 && rowWidth + gap + w > containerWidth) {
      flush(false)
    }
    row.push(item)
    rowWidth += (row.length === 1 ? 0 : gap) + w
  }
  flush(true)
  return rows
}

export function clampDensity(n: number): number {
  return Math.min(12, Math.max(2, Math.round(n)))
}

/** Altezza riservata per un bucket non ancora caricato (scrollbar stabile). */
export function bucketMinHeight(
  count: number,
  containerWidth: number,
  density: number
): number {
  const cols = Math.max(1, clampDensity(density))
  const rowH = Math.max(48, containerWidth / cols)
  return Math.ceil(Math.max(0, count) / cols) * rowH
}

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

/** Rows of constant height, widths proportional to aspect ratio. */
export function justify(
  items: AspectItem[],
  containerWidth: number,
  targetRowHeight: number
): JustifiedRow[] {
  if (containerWidth <= 0 || targetRowHeight <= 0 || items.length === 0) {
    return []
  }
  // GRID_GAP from the prototype (keeppix-mockup.html) — not an estimated
  // value: 4 had no basis in the mockup.
  const gap = 6
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

/** "Grid density": two distinct ranges, desktop 2-12 and mobile 2-6 —
 * this same function previously only served the desktop case
 * (`useDensity`, its only consumer before this, never used with a
 * different range). */
export function clampDensity(n: number, mobile = false): number {
  const max = mobile ? 6 : 12
  return Math.min(max, Math.max(2, Math.round(n)))
}

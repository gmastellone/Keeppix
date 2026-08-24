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
  // GRID_GAP del prototipo (keeppix-mockup.html riga 4593) — non un valore
  // stimato: 4 non aveva alcuna fonte nel mockup.
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

/** §60.2 "Densità griglia" (Task 14, 1/N): due intervalli distinti,
 * desktop 2-12 e mobile 2-6 — la stessa funzione serviva prima solo il
 * caso desktop (`useDensity`, unico consumatore fino a questo task, mai
 * usato su un intervallo diverso). */
export function clampDensity(n: number, mobile = false): number {
  const max = mobile ? 6 : 12
  return Math.min(max, Math.max(2, Math.round(n)))
}

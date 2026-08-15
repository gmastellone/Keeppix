/** Decodifica DC + luma AC di un thumbhash (hex) in data URL. */

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  }
  return out
}

function signed(n: number): number {
  return n > 7 ? n - 16 : n
}

function toByte(n: number): number {
  return Math.round(Math.min(1, Math.max(0, n)) * 255)
}

export function thumbhashToDataURL(hex: string): string | undefined {
  if (hex.length < 10) return undefined
  const hash = hexToBytes(hex)
  if (hash.length < 5) return undefined

  const header = hash[0]
  const lDc = (header & 63) / 63
  const pDc = ((hash[1] & 15) / 15) * 2 - 1
  const qDc = ((hash[1] >> 4) / 15) * 2 - 1
  const lScale = (hash[2] & 15) / 15
  const hasAlpha = (header >> 7) === 1
  const isLandscape = ((header >> 6) & 1) === 1
  const lx = isLandscape ? (hasAlpha ? 5 : 7) : hash[3] & 7
  const ly = isLandscape ? hash[3] & 7 : hasAlpha ? 5 : 7
  const w = isLandscape ? 32 : Math.max(1, Math.round((32 * lx) / Math.max(ly, 1)))
  const h = isLandscape ? Math.max(1, Math.round((32 * ly) / Math.max(lx, 1))) : 32
  const rgba = new Uint8Array(w * h * 4)

  let i = 0
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let l = lDc
      let j = hasAlpha ? 6 : 5
      for (let cy = 0; cy < ly; cy++) {
        const fy = Math.cos((Math.PI * y * cy) / h)
        for (let cx = cy === 0 ? 1 : 0; cx * ly < lx * (ly - cy); cx++) {
          const fx = Math.cos((Math.PI * x * cx) / w) * fy
          const byte = hash[Math.min(j >> 1, hash.length - 1)]
          const nibble = (j & 1) === 0 ? byte & 15 : byte >> 4
          l += (signed(nibble) / 8) * lScale * fx
          j++
        }
      }
      const b = l - (2 / 3) * pDc
      const r = (3 * l - b + qDc) / 2
      const g = r - qDc
      rgba[i++] = toByte(r)
      rgba[i++] = toByte(g)
      rgba[i++] = toByte(b)
      rgba[i++] = 255
    }
  }

  const canvas = document.createElement('canvas')
  canvas.width = w
  canvas.height = h
  const ctx = canvas.getContext('2d')
  if (!ctx) return undefined
  ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), 0, 0)
  return canvas.toDataURL()
}

/**
 * Decoder for the binary payload of `GET /timeline/geometry`: an 8-byte
 * header (u32 version, u32 count) followed by a 6-byte record per shot
 * (u16 width, u16 height, u16 month = year*12+month), all little-endian —
 * see `crates/keeppix-api/src/routes/timeline.rs::encode_geometry`, the
 * source of truth for the format.
 *
 * A `DataView` over the raw `ArrayBuffer`, not 214,000 `{w,h,month}`
 * objects: roughly 50 MB of heap and GC pressure on every scroll versus
 * 4.7 MB with no garbage generated.
 */
const SUPPORTED_FORMAT_VERSION = 1
const HEADER_BYTES = 8
const RECORD_BYTES = 6

export class UnsupportedGeometryFormatError extends Error {
  readonly version: number

  constructor(version: number) {
    super(`unsupported timeline geometry format version ${version}`)
    this.name = 'UnsupportedGeometryFormatError'
    this.version = version
  }
}

export class TimelineGeometry {
  readonly count: number
  private readonly view: DataView

  constructor(buffer: ArrayBuffer) {
    this.view = new DataView(buffer)
    const version = this.view.getUint32(0, true)
    if (version !== SUPPORTED_FORMAT_VERSION) {
      throw new UnsupportedGeometryFormatError(version)
    }
    this.count = this.view.getUint32(4, true)
  }

  width(index: number): number {
    return this.view.getUint16(HEADER_BYTES + index * RECORD_BYTES, true)
  }

  height(index: number): number {
    return this.view.getUint16(HEADER_BYTES + index * RECORD_BYTES + 2, true)
  }

  /** `year*12 + calendar_month (1..=12)`, the same index the backend uses. */
  month(index: number): number {
    return this.view.getUint16(HEADER_BYTES + index * RECORD_BYTES + 4, true)
  }

  /**
   * Merges buffers from multiple pages (cold-start loading) into a
   * single `TimelineGeometry` — one header with the summed count,
   * records concatenated in arrival order. Pages already arrive in order
   * (`taken_at_utc DESC, id DESC`, the same cursor used server-side), so
   * this only needs to copy bytes, without re-checking the ordering.
   */
  static concat(buffers: readonly ArrayBuffer[]): TimelineGeometry {
    if (buffers.length === 1) return new TimelineGeometry(buffers[0])
    let totalRecords = 0
    for (const buffer of buffers) {
      totalRecords += new DataView(buffer).getUint32(4, true)
    }
    const out = new Uint8Array(HEADER_BYTES + totalRecords * RECORD_BYTES)
    const outView = new DataView(out.buffer)
    outView.setUint32(0, SUPPORTED_FORMAT_VERSION, true)
    outView.setUint32(4, totalRecords, true)
    let offset = HEADER_BYTES
    for (const buffer of buffers) {
      const count = new DataView(buffer).getUint32(4, true)
      const recordBytes = count * RECORD_BYTES
      out.set(new Uint8Array(buffer, HEADER_BYTES, recordBytes), offset)
      offset += recordBytes
    }
    return new TimelineGeometry(out.buffer)
  }
}

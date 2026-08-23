/**
 * Decodificatore del payload binario di `GET /timeline/geometry` (Fase 11
 * Task 4, spec documento funzionale §66.9): intestazione da 8 byte
 * (versione u32, conteggio u32) seguita da un record da 6 byte per scatto
 * (larghezza u16, altezza u16, mese u16 = anno*12+mese), tutto little-endian
 * — vedi `crates/keeppix-api/src/routes/timeline.rs::encode_geometry`, la
 * fonte di verità del formato.
 *
 * Un `DataView` sull'`ArrayBuffer` grezzo, non 214.000 oggetti `{w,h,month}`:
 * la Ruling della spec (§3) è esplicita sul perché — ~50 MB di heap e
 * pressione sul GC ad ogni scroll contro 4,7 MB senza spazzatura.
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

  /** `anno*12 + mese_di_calendario (1..=12)`, lo stesso indice del backend. */
  month(index: number): number {
    return this.view.getUint16(HEADER_BYTES + index * RECORD_BYTES + 4, true)
  }
}

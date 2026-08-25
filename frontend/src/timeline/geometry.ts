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

  /**
   * Unisce i buffer di più pagine (Task 4-bis, caricamento a schermo
   * freddo) in una `TimelineGeometry` sola — un'unica intestazione con il
   * conteggio sommato, i record concatenati nell'ordine di arrivo. Le pagine
   * arrivano già in ordine (`taken_at_utc DESC, id DESC`, lo stesso cursore
   * lato server), quindi qui basta copiare i byte, senza ricontrollare
   * l'ordinamento.
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

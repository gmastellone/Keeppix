// Fase 11 — sottosistema di caricamento (`docs/ui/caricamento-nuove-foto.md`
// §4, "Cosa entra e cosa no"), verificato riga per riga contro la tabella
// esatta delle estensioni (righe 95-102).
//
// `dng` è trattato come RAW, non come immagine — un contenitore RAW a tutti
// gli effetti (§4, nota dopo la tabella), non un'eccezione dimenticata.
const IMAGE_EXTENSIONS = new Set(['jpg', 'jpeg', 'jpe', 'png', 'tif', 'tiff', 'webp', 'heic', 'heif'])
const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'm4v'])
const RAW_EXTENSIONS = new Set([
  'arw', 'sr2', 'srf', 'cr2', 'cr3', 'crw', 'nef', 'nrw', 'raf', 'orf', 'rw2', 'raw', 'dng',
  'pef', 'srw', 'x3f', '3fr', 'iiq', 'mos', 'mef', 'erf', 'kdc', 'dcr', 'mrw', 'rwl', 'fff'
])

export type FileCategory = 'image' | 'video' | 'raw' | 'unsupported'

function extensionOf(filename: string): string {
  const dot = filename.lastIndexOf('.')
  if (dot < 0 || dot === filename.length - 1) return ''
  return filename.slice(dot + 1).toLowerCase()
}

export function categorize(filename: string): FileCategory {
  const ext = extensionOf(filename)
  if (IMAGE_EXTENSIONS.has(ext)) return 'image'
  if (VIDEO_EXTENSIONS.has(ext)) return 'video'
  if (RAW_EXTENSIONS.has(ext)) return 'raw'
  return 'unsupported'
}

export interface ClassifiedFiles {
  /** Immagini e video: quello che parte subito (§4, "quello che può entrare
   * parte subito"). */
  accepted: File[]
  /** RAW, con rimando al Culling — mai un errore silenzioso (§4.1). */
  rejectedRaw: File[]
  /** Tutto il resto, formato non supportato. */
  rejectedUnsupported: File[]
}

/**
 * Divide un rilascio/selezione senza mai rifiutare l'intero gruppo per la
 * presenza di RAW o formati ignoti (§4: "Rifiutare l'intero rilascio
 * sarebbe ostile e gli farebbe perdere il lavoro buono insieme a quello
 * scartato").
 */
export function classifyFiles(files: File[]): ClassifiedFiles {
  const accepted: File[] = []
  const rejectedRaw: File[] = []
  const rejectedUnsupported: File[] = []

  for (const file of files) {
    const category = categorize(file.name)
    if (category === 'image' || category === 'video') accepted.push(file)
    else if (category === 'raw') rejectedRaw.push(file)
    else rejectedUnsupported.push(file)
  }

  return { accepted, rejectedRaw, rejectedUnsupported }
}

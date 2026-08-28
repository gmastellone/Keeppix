// Upload subsystem (`docs/ui/caricamento-nuove-foto.md`, "What comes in
// and what doesn't"), verified against the exact extension table.
//
// `dng` is treated as RAW, not as an image — a RAW container in every
// meaningful sense, not a forgotten exception.
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
  /** Images and video: whatever can go in starts uploading immediately. */
  accepted: File[]
  /** RAW files, pointed toward Culling instead — never a silent error. */
  rejectedRaw: File[]
  /** Everything else: an unsupported format. */
  rejectedUnsupported: File[]
}

/**
 * Splits a drop/selection without ever rejecting the whole group just
 * because it contains RAW files or unknown formats — rejecting the
 * entire drop would be hostile to the user, throwing away the good work
 * along with the discarded files.
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

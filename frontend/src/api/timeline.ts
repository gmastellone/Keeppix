import { apiFetch, throwProblem } from './client'

export interface MonthBucket {
  month: string
  count: number
}

/** Un tag confermato (Fase 11 Task 7, SP-3 §11 — dimensioni "Tag"/
 * "Categorie"). `category_id` è il `parent_id` del tag: le "categorie" del
 * documento sono tag con `kind='category'`, non un secondo concetto. */
export interface AssetTagBadge {
  id: string
  name: string
  color: string | null
  category_id: string | null
}

/** Un volto confermato (Fase 11 Task 7, SP-3 §11 — dimensione "Persone"). */
export interface AssetFaceBadge {
  person_id: string
  person_name: string | null
}

/** EXIF completo (Fase 11 Task 8, §19.2 campi 6-9, sezione "SCATTO" del
 * lightbox) — a differenza di `camera_model` (una stringa sola, SP-3),
 * presente **solo** sulla risposta di `GET /assets/{id}` (dettaglio
 * singolo): il backend non lo calcola su `/timeline`/`/search`, un giro di
 * query in più per riga che nessuna griglia legge. */
export interface AssetExifDetail {
  camera_make: string | null
  camera_model: string | null
  lens: string | null
  iso: number | null
  f_number: number | null
  exposure: string | null
  focal_length: number | null
}

export interface TimelineAsset {
  id: string
  folder_id: string
  filename: string
  content_hash: string | null
  size_bytes: number
  kind: string
  status: string
  taken_at_utc: string | null
  width: number | null
  height: number | null
  thumbhash: string | null
  /** SP-15 (`AssetView.raw_kind`): `"raw"` / `"jpeg"` / `"raw+jpeg"`, `null`
   * per un kind che non è né l'uno né l'altro (video, unknown). */
  raw_kind: string | null
  favorite: boolean
  /** SP-3 §11, dimensione "Fotocamera" — `null` se l'exif non porta il
   * modello o non esiste affatto. Campo additivo, Fase 11 Task 7. */
  camera_model: string | null
  /** SP-3 §11 — solo tag confermati, mai proposte in attesa. Sempre un
   * array, mai assente (`[]` se non ne ha). Campo additivo, Fase 11 Task 7. */
  tags: AssetTagBadge[]
  /** SP-3 §11 — solo volti confermati. Sempre un array, mai assente. Campo
   * additivo, Fase 11 Task 7. */
  faces: AssetFaceBadge[]
  /** Fase 11 Task 8, §19.2 sezione "SCATTO" — presente **solo** quando
   * l'oggetto viene da `GET /assets/{id}` (il lightbox), assente (non
   * `null`) su ogni asset di `/timeline`/`/search`. Campo opzionale di
   * proposito: `photo()` nei test esistenti non deve cambiare. */
  full_exif?: AssetExifDetail
  /** Posizione effettiva (`COALESCE(override, exif)`), stessa fonte di
   * `metadata.ts#AssetMetadata.location` — presente solo quando l'oggetto
   * viene da `GET /assets/{id}` (mai da `/timeline`/`/search`, stesso
   * motivo di `full_exif`). Fase 11 Task 8, §19.2 sezione "POSIZIONE". */
  location?: { lat: number; lon: number } | null
  place_id?: number | null
  /** 1 se non impilato, altrimenti il numero di file della pila (RAW+JPEG
   * incluso). Fase 11 Task 8, §19.2 sezione "SCATTO"/commutatore RAW-JPEG. */
  stack_size?: number
}

export interface TimelinePage {
  assets: TimelineAsset[]
  next_cursor?: string
}

/** `GET /assets/{id}` — l'unico posto che popola `full_exif`/`location`/
 * `place_id`/`stack_size` (assenti su ogni asset di `/timeline`/`/search`).
 * Usato dal pannello informazioni del lightbox (§19), mai dalle griglie. */
export function fetchAsset(id: string): Promise<TimelineAsset> {
  return apiFetch(`/api/v1/assets/${id}`)
}

export function fetchBuckets(bbox?: string): Promise<MonthBucket[]> {
  const query = bbox ? `?${new URLSearchParams({ bbox })}` : ''
  return apiFetch(`/api/v1/timeline/buckets${query}`)
}

export function fetchPage(bucket: string, cursor?: string, bbox?: string): Promise<TimelinePage> {
  const q = new URLSearchParams({ bucket })
  if (cursor) q.set('cursor', cursor)
  if (bbox) q.set('bbox', bbox)
  return apiFetch(`/api/v1/timeline?${q}`)
}

export function promoteViewport(hashes: string[]): Promise<null> {
  return apiFetch('/api/v1/viewport', {
    method: 'POST',
    body: JSON.stringify({ hashes })
  })
}

export interface GeometryResponse {
  /** `null` su 304: il chiamante tiene la geometria già decodificata. */
  buffer: ArrayBuffer | null
  etag: string | null
}

/**
 * `GET /timeline/geometry` (Fase 11 Task 4) risponde `application/
 * octet-stream`, non JSON — non può passare da `apiFetch`. `etag`, se
 * passato, va in `If-None-Match`: un `304` restituisce `buffer: null`
 * invece di ri-scaricare ~4,7 MB per una vista invariata.
 */
export async function fetchGeometry(bbox?: string, etag?: string): Promise<GeometryResponse> {
  const query = bbox ? `?${new URLSearchParams({ bbox })}` : ''
  const headers: Record<string, string> = { 'x-keeppix-client': 'web' }
  if (etag) headers['if-none-match'] = etag
  const response = await fetch(`/api/v1/timeline/geometry${query}`, {
    credentials: 'same-origin',
    headers
  })
  if (response.status === 304) {
    return { buffer: null, etag: etag ?? null }
  }
  if (!response.ok) {
    await throwProblem(response)
  }
  return { buffer: await response.arrayBuffer(), etag: response.headers.get('etag') }
}

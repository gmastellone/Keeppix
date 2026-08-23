import { apiFetch, throwProblem } from './client'

export interface MonthBucket {
  month: string
  count: number
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
}

export interface TimelinePage {
  assets: TimelineAsset[]
  next_cursor?: string
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

import { apiFetch, throwProblem } from './client'

export interface MonthBucket {
  month: string
  count: number
}

/** A confirmed tag. `category_id` is the tag's `parent_id`: "categories"
 * are just tags with `kind='category'`, not a second concept. */
export interface AssetTagBadge {
  id: string
  name: string
  color: string | null
  category_id: string | null
}

/** A confirmed face. */
export interface AssetFaceBadge {
  person_id: string
  person_name: string | null
}

/** Full EXIF (the lightbox's "SHOT" section) — unlike `camera_model` (a
 * single string), present **only** in the `GET /assets/{id}` response
 * (single-asset detail): the backend doesn't compute it on
 * `/timeline`/`/search`, an extra per-row query that no grid reads. */
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
  /** (`AssetView.raw_kind`): `"raw"` / `"jpeg"` / `"raw+jpeg"`, `null`
   * for a kind that's neither (video, unknown). */
  raw_kind: string | null
  favorite: boolean
  /** `null` if the exif doesn't carry the model or doesn't exist at all. */
  camera_model: string | null
  /** Confirmed tags only, never pending proposals. Always an array,
   * never absent (`[]` if it has none). */
  tags: AssetTagBadge[]
  /** Confirmed faces only. Always an array, never absent. */
  faces: AssetFaceBadge[]
  /** Present **only** when the object comes from `GET /assets/{id}`
   * (the lightbox), absent (not `null`) on every asset from
   * `/timeline`/`/search`. Deliberately optional: `photo()` in existing
   * tests must not change. */
  full_exif?: AssetExifDetail
  /** Effective location (`COALESCE(override, exif)`), same source as
   * `metadata.ts#AssetMetadata.location` — present only when the object
   * comes from `GET /assets/{id}` (never from `/timeline`/`/search`,
   * same reason as `full_exif`). */
  location?: { lat: number; lon: number } | null
  place_id?: number | null
  /** 1 if not stacked, otherwise the number of files in the stack
   * (RAW+JPEG included). */
  stack_size?: number
}

export interface TimelinePage {
  assets: TimelineAsset[]
  next_cursor?: string
}

/** `GET /assets/{id}` — the only place that populates
 * `full_exif`/`location`/`place_id`/`stack_size` (absent on every asset
 * from `/timeline`/`/search`). Used by the lightbox's info panel, never
 * by the grids. */
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

/** Point lookup for a live-triggered refresh (`assets.upserted` over the
 * WebSocket carries exactly this `ids` list): patches already-rendered
 * tiles in place instead of refetching and redrawing the whole page they
 * live on. An id that's missing, not visible, or no longer indexed is
 * simply absent from the response, not an error — the caller only wants
 * back what it can still show. */
export function fetchAssetsByIds(ids: string[]): Promise<TimelineAsset[]> {
  return apiFetch<{ assets: TimelineAsset[] }>('/api/v1/timeline/by-ids', {
    method: 'POST',
    body: JSON.stringify({ ids })
  }).then((page) => page.assets)
}

export function promoteViewport(hashes: string[]): Promise<null> {
  return apiFetch('/api/v1/viewport', {
    method: 'POST',
    body: JSON.stringify({ hashes })
  })
}

export interface GeometryResponse {
  /** `null` on 304: the caller keeps the already-decoded geometry. */
  buffer: ArrayBuffer | null
  etag: string | null
  /** Present only on a paginated response (see `limit` below) when
   * there's more after it: pass it as `cursor` on the next request,
   * without interpreting it. Absent → this response was already
   * everything there is. */
  nextCursor: string | null
}

/**
 * `GET /timeline/geometry` responds with `application/octet-stream`, not
 * JSON — it can't go through `apiFetch`. `etag`, if passed, goes into
 * `If-None-Match`: a `304` returns `buffer: null` instead of
 * re-downloading ~4.7 MB for an unchanged view — only on the whole-view
 * request (`limit` absent): a paginated request never validates against
 * the `ETag`.
 *
 * `limit`/`cursor` exist for the initial cold-screen load (3.4s measured
 * on a slow network with 214,000 shots, over the 2s budget) — the "first
 * page to draw, then the rest in the background" loop lives in
 * `TimelineView.vue::refreshTimeline`, not here: that's view behavior,
 * not HTTP-client behavior.
 */
export async function fetchGeometry(
  bbox?: string,
  etag?: string,
  page?: { limit: number; cursor?: string }
): Promise<GeometryResponse> {
  const params = new URLSearchParams()
  if (bbox) params.set('bbox', bbox)
  if (page) {
    params.set('limit', String(page.limit))
    if (page.cursor) params.set('cursor', page.cursor)
  }
  const query = params.size > 0 ? `?${params}` : ''
  const headers: Record<string, string> = { 'x-keeppix-client': 'web' }
  if (etag && !page) headers['if-none-match'] = etag
  const response = await fetch(`/api/v1/timeline/geometry${query}`, {
    credentials: 'same-origin',
    headers
  })
  if (response.status === 304) {
    return { buffer: null, etag: etag ?? null, nextCursor: null }
  }
  if (!response.ok) {
    await throwProblem(response)
  }
  return {
    buffer: await response.arrayBuffer(),
    etag: response.headers.get('etag'),
    nextCursor: response.headers.get('x-keeppix-geometry-cursor')
  }
}

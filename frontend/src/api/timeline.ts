import { apiFetch } from './client'

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

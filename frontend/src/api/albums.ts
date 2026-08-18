import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

export interface Album {
  id: string
  name: string
  asset_count: number
  cover_hash: string | null
  created_at: string
}

export interface AlbumDetail {
  id: string
  name: string
  assets: TimelineAsset[]
}

export function fetchAlbums(): Promise<Album[]> {
  return apiFetch('/api/v1/albums')
}

export function fetchAlbum(id: string): Promise<AlbumDetail> {
  return apiFetch(`/api/v1/albums/${id}`)
}

export function createAlbum(name: string): Promise<Album> {
  return apiFetch('/api/v1/albums', {
    method: 'POST',
    body: JSON.stringify({ name })
  })
}

export function deleteAlbum(id: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${id}`, { method: 'DELETE' })
}

export function addAssets(albumId: string, assetIds: string[]): Promise<null> {
  return apiFetch(`/api/v1/albums/${albumId}/assets`, {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds })
  })
}

export function removeAsset(albumId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${albumId}/assets/${assetId}`, {
    method: 'DELETE'
  })
}

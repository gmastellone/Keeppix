import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

export interface Album {
  id: string
  name: string
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

/** Aggiunge più asset a un album — `POST /albums/{id}/assets/{asset_id}`
 * (`routes/albums.rs::add_asset`) prende **un** id alla volta, verificato
 * sul backend reale: nessun endpoint batch esiste su questo percorso
 * (`GET /albums/{id}/assets` è tutt'altro, l'elenco dei membri). Bug reale
 * trovato qui — la versione precedente postava un corpo `{asset_ids}` a
 * un URL che accetta solo GET — corretto in un ciclo sequenziale sullo
 * stesso endpoint singolo, stesso principio di `stores/favorites.ts`'s
 * `setMany`. */
export async function addAssets(albumId: string, assetIds: string[]): Promise<null> {
  for (const assetId of assetIds) {
    await apiFetch(`/api/v1/albums/${albumId}/assets/${assetId}`, { method: 'POST' })
  }
  return null
}

export function removeAsset(albumId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${albumId}/assets/${assetId}`, {
    method: 'DELETE'
  })
}

/** Un album di cui un asset è già membro (Fase 11 Task 8, §19.2 campo
 * 18) — solo id e nome, i chip non sono cliccabili. */
export interface AlbumBadge {
  id: string
  name: string
}

/** §19.2 sezione ALBUM del lightbox: la freccia opposta di `fetchAlbum` —
 * dato un asset, a quali album appartiene già (manuali e dinamici
 * insieme, materializzati sullo stesso `album_assets`). */
export function fetchAlbumsForAsset(assetId: string): Promise<AlbumBadge[]> {
  return apiFetch(`/api/v1/assets/${assetId}/albums`)
}

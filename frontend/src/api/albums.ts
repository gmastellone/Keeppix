import { apiFetch } from './client'
import type { SearchNode } from '@/search/ast'
import type { TimelineAsset } from './timeline'

/** Rispecchia `AlbumView` (`crates/keeppix-api/src/routes/albums.rs:20
 * -44`). Bug reale trovato e corretto qui (Task 12 1/N): il tipo
 * precedente dichiarava `cover_hash` (mai esistito sul backend, sempre
 * `undefined` a runtime) invece di `cover_asset_id`, e `fetchAlbum()`
 * era tipizzato come se `GET /albums/{id}` restituisse anche `assets` —
 * non li restituisce mai (verificato leggendo per intero il gestore
 * `routes::albums::get`): l'elenco membri vive solo su
 * `GET /albums/{id}/assets`, una rotta separata mai chiamata dal
 * frontend fino a questa unità. `AlbumPickerDialog.vue` copriva il
 * buco con `detail?.assets ?? []`, che quindi restituiva sempre `[]`
 * in produzione — l'appartenenza mostrata dal picker non rispecchiava
 * mai la realtà del server. */
export interface Album {
  id: string
  name: string
  description: string
  owner_id: string
  cover_asset_id?: string
  created_at: string
  updated_at: string
  /** Presente solo se l'album può essere aggiornato con `POST .../
   * refresh` (nessun album "dinamico" che si aggiorna da solo — un
   * filtro che si rilancia quando lo chiede l'utente, Fase 10 Task 5). */
  rule?: SearchNode
  rule_run_at?: string
  is_shared: boolean
  /** Sempre assente in pratica: nessuna rotta permette di impostarlo
   * (`PatchAlbumBody` non ha un campo `cover_tint`/`monochrome`) — la
   * copertina a gradiente di §41/§42 è quindi calcolata lato client,
   * deterministica sull'id, non letta da qui. Il campo resta tipizzato
   * per completezza e per il giorno in cui il backend lo popolasse. */
  cover_tint?: string
  monochrome: boolean
}

/** Un membro di album come lo restituisce `GET /albums/{id}/assets`
 * (`AlbumAssetView`, `#[serde(flatten)]` di `AssetView` + tre campi
 * propri) — un sovrainsieme di `TimelineAsset`, i campi extra vengono
 * ignorati da chi si aspetta solo quello. */
export interface AlbumAsset extends TimelineAsset {
  position: number
  added_by: string
  added_at: string
}

export function fetchAlbums(): Promise<Album[]> {
  return apiFetch('/api/v1/albums')
}

export function fetchAlbum(id: string): Promise<Album> {
  return apiFetch(`/api/v1/albums/${id}`)
}

/** §42, il contenuto reale di un album — mai chiamata prima di questa
 * unità (vedi il commento su `Album` sopra per il bug che copriva). */
export function fetchAlbumAssets(id: string): Promise<AlbumAsset[]> {
  return apiFetch(`/api/v1/albums/${id}/assets`)
}

export function createAlbum(name: string, rule?: SearchNode): Promise<Album> {
  return apiFetch('/api/v1/albums', {
    method: 'POST',
    body: JSON.stringify(rule ? { name, rule } : { name })
  })
}

export function deleteAlbum(id: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${id}`, { method: 'DELETE' })
}

/** §43, "Aggiorna album": rilancia la `rule` con cui l'album è nato —
 * la controparte reale, manuale, dell'"Automatico" del mockup (che
 * vorrebbe una ricomputazione continua mai esistita in questo backend,
 * solo un filtro che si rilancia su richiesta — Fase 10 Task 5).
 * Risponde con lo stesso `BulkOutcome` di tutte le altre operazioni di
 * massa (`crates/keeppix-api/src/bulk.rs`) riusato qui in un modo
 * insolito: `succeeded` è la concatenazione degli id entrati **e**
 * usciti dall'album, senza distinguere quali — il gestore
 * (`routes::albums::refresh`) li unisce prima di rispondere. Nessun
 * conteggio separato "aggiunte"/"rimosse" è quindi possibile da qui. */
export function refreshAlbum(id: string): Promise<{ succeeded: string[] }> {
  return apiFetch(`/api/v1/albums/${id}/refresh`, { method: 'POST' })
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

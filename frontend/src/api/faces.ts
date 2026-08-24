import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

/** Fase 11 Task 8 (5/N), §19.2 sezione "PERSONE" e §18.2 (riquadri volto
 * sull'immagine): primo consumatore frontend di queste rotte — costruite
 * in Fase 8, mai chiamate da questa app finora (`AssetFaceBadge`, già in
 * uso da SP-3, porta solo `person_id`/`person_name`: mai il riquadro). */
export interface FaceBBox {
  x: number
  y: number
  w: number
  h: number
}

export interface Face {
  id: string
  asset_id: string
  bbox: FaceBBox
  person_id: string | null
  proposed_person_id: string | null
  proposed_score: number | null
  assigned_by_human: boolean
}

export function fetchFacesForAsset(assetId: string): Promise<Face[]> {
  return apiFetch(`/api/v1/assets/${assetId}/faces`)
}

/** Assegnazione manuale — sia "Correggi persona…" (un volto già confermato
 * su un'altra persona) sia, in futuro, "+ aggiungi" (volto appena creato):
 * `personId` deve già esistere (creata con `persons.ts#createPerson` se è
 * una persona nuova). */
export function assignFace(faceId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/faces/${faceId}/assign`, {
    method: 'POST',
    body: JSON.stringify({ person_id: personId })
  })
}

/** "Non è un volto" — falso positivo permanente (§19.3): non torna mai più
 * proposto, a differenza di un `assign` che si può sempre correggere di
 * nuovo. */
export function rejectFace(faceId: string): Promise<null> {
  return apiFetch(`/api/v1/faces/${faceId}/reject`, { method: 'POST' })
}

/** Fase 11 Task 16 (4/N), §33/§36: "una miniatura per ogni volto
 * confermato della persona" (§33.2), non per ogni foto — se una persona
 * ha due volti confermati nella stessa foto compaiono due miniature
 * (§36.2, esplicito). Nessuna rotta lista i volti di una persona
 * direttamente (`GET /assets/{id}/faces` richiede già un asset noto):
 * un giro per ciascuna delle foto già caricate dal chiamante
 * (`PersonDetailView.vue`, via `runSearch({op:'person'})`), filtrato su
 * `person_id`. Costo N accettato, stesso principio di `ReviewView.vue`. */
export interface PersonFaceTile {
  asset: TimelineAsset
  face: Face
}

export async function fetchPersonFaceTiles(personId: string, assets: TimelineAsset[]): Promise<PersonFaceTile[]> {
  const perAsset = await Promise.all(
    assets.map(async (asset) => {
      const faces = await fetchFacesForAsset(asset.id)
      return faces.filter((face) => face.person_id === personId).map((face) => ({ asset, face }))
    })
  )
  return perAsset.flat()
}

/** §60.8 "Elimina tutti i dati dei volti" — reale e irreversibile, solo
 * admin (`routes/faces.rs::delete_all_data`): svuota volti, persone e
 * gruppi di persone in tutta l'istanza. Non tocca le foto. */
export function deleteAllFaceData(): Promise<null> {
  return apiFetch('/api/v1/faces/data', { method: 'DELETE' })
}

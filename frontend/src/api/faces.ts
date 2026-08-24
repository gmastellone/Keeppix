import { apiFetch } from './client'

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

/** §60.8 "Elimina tutti i dati dei volti" — reale e irreversibile, solo
 * admin (`routes/faces.rs::delete_all_data`): svuota volti, persone e
 * gruppi di persone in tutta l'istanza. Non tocca le foto. */
export function deleteAllFaceData(): Promise<null> {
  return apiFetch('/api/v1/faces/data', { method: 'DELETE' })
}

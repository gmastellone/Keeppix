import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

/** Face bounding boxes for the "PEOPLE" section and the face rectangles
 * drawn on the image (`AssetFaceBadge`, already used elsewhere, only
 * carries `person_id`/`person_name` — never the bounding box). */
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

/** Manual assignment — either "Correct person…" (a face already
 * confirmed on another person) or, in the future, "+ add" (a freshly
 * created face): `personId` must already exist (created with
 * `persons.ts#createPerson` if it's a new person). */
export function assignFace(faceId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/faces/${faceId}/assign`, {
    method: 'POST',
    body: JSON.stringify({ person_id: personId })
  })
}

/** "Not a face" — a permanent false positive: it's never proposed again,
 * unlike an `assign` which can always be corrected later. */
export function rejectFace(faceId: string): Promise<null> {
  return apiFetch(`/api/v1/faces/${faceId}/reject`, { method: 'POST' })
}

/** "Review — faces" queue: proposed faces (uncertain assignment), the
 * same flat-ordered-by-score pattern as `tags.ts#fetchTagProposals` — no
 * route groups by suggested person, grouping happens client-side in
 * `ReviewView.vue` via `Face.proposed_person_id`. */
export function fetchFaceProposals(): Promise<Face[]> {
  return apiFetch('/api/v1/faces/proposals')
}

/** Single "Confirm" — `personId = proposed_person_id` (already the
 * proposed person, no need to pass it: the route reads it from the face
 * itself, unlike `assignFace`). */
export function confirmFaceProposal(faceId: string): Promise<null> {
  return apiFetch(`/api/v1/faces/${faceId}/confirm`, { method: 'POST' })
}

/** "Confirm all", per suggested person. */
export function confirmAllFaceProposals(personId: string): Promise<null> {
  return apiFetch(`/api/v1/persons/${personId}/proposals/confirm`, { method: 'POST' })
}

/** One thumbnail per confirmed face of the person, not per photo — if a
 * person has two confirmed faces in the same photo, two thumbnails show
 * up. No route lists a person's faces directly (`GET /assets/{id}/faces`
 * already requires a known asset): one round-trip per photo already
 * loaded by the caller (`PersonDetailView.vue`, via
 * `runSearch({op:'person'})`), filtered on `person_id`. The O(N) cost is
 * accepted, same approach as `ReviewView.vue`. */
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

/** "Delete all face data" — real and irreversible, admin only
 * (`routes/faces.rs::delete_all_data`): clears faces, people, and person
 * groups instance-wide. Does not touch the photos. */
export function deleteAllFaceData(): Promise<null> {
  return apiFetch('/api/v1/faces/data', { method: 'DELETE' })
}

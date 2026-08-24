import { apiFetch } from './client'

// Fase 11 Task 7 (SP-3 §11, dimensioni "Tag"/"Categorie"): primo consumatore
// frontend di queste rotte — costruite in Fase 7, mai chiamate da questa
// app finora. `GET /tags` restituisce tag **e** categorie insieme
// (`kind: 'tag' | 'category'`), non due elenchi separati: la distinzione è
// sul campo, filtrata qui dal chiamante — stesso comportamento della rotta,
// non un'invenzione del frontend.
export interface Tag {
  id: string
  name: string
  kind: 'tag' | 'category'
  parent_id: string | null
  color: string | null
  assignment_count: number
}

export function fetchTags(): Promise<Tag[]> {
  return apiFetch('/api/v1/tags')
}

/** "Aggiungi tag…" di Modifica in blocco (§13.3 campo 5): assegna il tag a
 * ogni asset della selezione, `source='user'` — un'aggiunta manuale è già
 * una conferma, non passa dalla coda di revisione (SP-12). */
export function assignTagBatch(tagId: string, assetIds: string[]): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/batch`, {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds })
  })
}

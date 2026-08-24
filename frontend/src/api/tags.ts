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

/** La freccia opposta: "attiva/disattiva un tag per aggiungerlo o
 * toglierlo da tutti" (dialog di scelta tag, §13.3 campo 5, verificato sul
 * prototipo reale — `openTagPickerDialog` in `docs/ui/keeppix-mockup.html`
 * chiama sia `addManualTag` sia `removeTagFromPhoto` dallo stesso
 * controllo). */
export function unassignTagBatch(tagId: string, assetIds: string[]): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/batch/remove`, {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds })
  })
}

/** Un tag come lo mostra il pannello informazioni del lightbox (Fase 11
 * Task 8, §19.2 campi 14-17): `state`/`source` grezzi — confermato/in
 * attesa (mai rifiutato, già filtrato dal backend), IA/umano. */
export interface AssetTagDetail {
  id: string
  name: string
  color: string | null
  category_id: string | null
  state: 'confirmed' | 'proposed'
  source: 'ai' | 'user'
}

/** §19.2 sezione TAG: tag confermati e in attesa di **un solo** asset —
 * primo consumatore di questa rotta, costruita per il lightbox. */
export function fetchTagsForAsset(assetId: string): Promise<AssetTagDetail[]> {
  return apiFetch(`/api/v1/assets/${assetId}/tags`)
}

/** §19.3, la `×` sui chip confermati: rimuove permanentemente un tag già
 * confermato (transizione a `'rejected'`, mai una `DELETE` — vedi
 * `AssetTagRepo::remove_confirmed` sul backend per il perché). */
export function removeConfirmedTag(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/remove`, { method: 'POST' })
}

/** §19.2 sezione "In attesa di conferma": il `✓` su una proposta —
 * transita `state: 'proposed' → 'confirmed'` (`AssetTagRepo::confirm`, la
 * stessa macchina a stati a senso unico di `remove_confirmed`). Nessun
 * wrapper esisteva ancora: la coda di revisione globale (fuori campo qui)
 * e il lightbox sono i primi due consumatori reali di questa rotta. */
export function confirmTagProposal(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/confirm`, { method: 'POST' })
}

/** Il `×` su una proposta (SP-10): `state: 'proposed' → 'rejected'`,
 * permanente — a differenza di `removeConfirmedTag`, qui la proposta non
 * era mai stata confermata: non c'è nulla da "rimuovere", solo da
 * rifiutare prima che diventi un tag vero. */
export function rejectTagProposal(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/reject`, { method: 'POST' })
}

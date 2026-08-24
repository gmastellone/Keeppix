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
  /** Opaco: qualunque stringa CSS `color` valida (hex nei test reali,
   * `crates/keeppix-db/tests/tags.rs:77`) — non la tinta HSL 0-360 pura
   * che il documento descrive. `TagPickerDialog.vue` la usa già così, come
   * `background` diretto: le 10 pastiglie dell'editor (Task 15 1/N)
   * scrivono `hsl(H,60%,50%)` per intero, non solo `H`. */
  color: string | null
  /** Assente su una categoria (il backend la restituisce solo per
   * `kind==='tag'`, `#[serde(skip_serializing_if)]`). */
  prompt?: string
  threshold?: number
  assignment_count: number
}

export function fetchTags(): Promise<Tag[]> {
  return apiFetch('/api/v1/tags')
}

export interface TagPayload {
  name: string
  kind: 'tag' | 'category'
  parent_id?: string | null
  prompt?: string | null
  color?: string | null
  threshold?: number
}

/** §52.3 "Nuovo tag"/"Nuova categoria", §53-54: crea un tag o una
 * categoria — stesso endpoint, distinto da `kind`. Un tag con `name`/
 * `prompt` non vuoti fa calcolare subito l'embedding testuale sul server
 * (se il modello è presente) e propone l'abbinamento sulle foto già
 * indicizzate — non tocca nulla se il modello manca (`has_embedding:
 * false` nella risposta, mai un errore). */
export function createTag(payload: TagPayload): Promise<Tag> {
  return apiFetch('/api/v1/tags', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export interface TagPatchPayload {
  name?: string
  parent_id?: string | null
  prompt?: string | null
  color?: string | null
  threshold?: number
}

/** §53.3 "Salva": `parent_id`/`prompt`/`color` sono "assente = invariato,
 * `null` = azzera" sul backend (`PatchTagRequest`, `Option<Option<T>>`) —
 * per azzerare la categoria di un tag va passato `parent_id: null` per
 * davvero, non omesso. */
export function patchTag(id: string, payload: TagPatchPayload): Promise<Tag> {
  return apiFetch(`/api/v1/tags/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(payload)
  })
}

/** §52.3 punto 4/§54: cancella un tag o una categoria. Su un tag, elimina
 * a cascata ogni riga `asset_tags` (FK `ON DELETE CASCADE`) — il numero
 * mostrato nel dialog di conferma è `Tag.assignment_count`, già nella
 * risposta di `fetchTags()`, non una chiamata a parte. Su una categoria,
 * i tag al suo interno restano: solo `parent_id` si azzera (FK `ON DELETE
 * SET NULL`), mai una cancellazione a catena. */
export function deleteTag(id: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${id}`, { method: 'DELETE' })
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

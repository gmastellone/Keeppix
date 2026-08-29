import { apiFetch } from './client'

// `GET /tags` returns tags **and** categories together
// (`kind: 'tag' | 'category'`), not two separate lists: the distinction
// is on the field, filtered here by the caller — that's the route's
// actual behavior, not something invented by the frontend.
export interface Tag {
  id: string
  name: string
  kind: 'tag' | 'category'
  parent_id: string | null
  /** Opaque: any valid CSS `color` string (hex in the real tests,
   * `crates/keeppix-db/tests/tags.rs:77`), not a bare 0-360 HSL hue.
   * `TagPickerDialog.vue` already uses it this way, as a direct
   * `background` value: the editor's 10 swatches write the full
   * `hsl(H,60%,50%)`, not just `H`. */
  color: string | null
  /** Absent on a category (the backend only returns it for
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

/** Creates a tag or a category — same endpoint, distinguished by `kind`.
 * A tag with non-empty `name`/`prompt` immediately triggers a
 * server-side text embedding computation (if the model is present) and
 * proposes matches on already-indexed photos — it doesn't touch
 * anything if the model is missing (`has_embedding: false` in the
 * response, never an error). */
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

/** `parent_id`/`prompt`/`color` are "absent = unchanged, `null` = clear"
 * on the backend (`PatchTagRequest`, `Option<Option<T>>`) — clearing a
 * tag's category requires actually passing `parent_id: null`, not
 * omitting it. */
export function patchTag(id: string, payload: TagPatchPayload): Promise<Tag> {
  return apiFetch(`/api/v1/tags/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(payload)
  })
}

/** Deletes a tag or a category. For a tag, cascades deletion to every
 * `asset_tags` row (FK `ON DELETE CASCADE`) — the count shown in the
 * confirmation dialog is `Tag.assignment_count`, already in
 * `fetchTags()`'s response, not a separate call. For a category, the
 * tags inside it remain: only `parent_id` gets cleared (FK `ON DELETE
 * SET NULL`), never a cascading delete. */
export function deleteTag(id: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${id}`, { method: 'DELETE' })
}

/** "Add tag…" from the bulk-edit menu: assigns the tag to every asset in
 * the selection, `source='user'` — a manual addition is already a
 * confirmation, it doesn't go through the review queue. */
export function assignTagBatch(tagId: string, assetIds: string[]): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/batch`, {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds })
  })
}

/** The reverse: toggling a tag on the picker adds or removes it from
 * every selected asset from the same control. */
export function unassignTagBatch(tagId: string, assetIds: string[]): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/batch/remove`, {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds })
  })
}

/** A tag as shown by the lightbox's info panel: raw `state`/`source` —
 * confirmed/pending (never rejected, already filtered by the backend),
 * AI/human. */
export interface AssetTagDetail {
  id: string
  name: string
  color: string | null
  category_id: string | null
  state: 'confirmed' | 'proposed'
  source: 'ai' | 'user'
}

/** Confirmed and pending tags for **one** asset — built for the
 * lightbox. */
export function fetchTagsForAsset(assetId: string): Promise<AssetTagDetail[]> {
  return apiFetch(`/api/v1/assets/${assetId}/tags`)
}

/** The `×` on confirmed chips: permanently removes an already-confirmed
 * tag (transitions to `'rejected'`, never a `DELETE` — see
 * `AssetTagRepo::remove_confirmed` on the backend for why). */
export function removeConfirmedTag(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/remove`, { method: 'POST' })
}

/** The `✓` on a proposal — transitions `state: 'proposed' → 'confirmed'`
 * (`AssetTagRepo::confirm`, the same one-way state machine as
 * `remove_confirmed`). Used by the lightbox and by the global Review
 * queue (checkmark on a single thumbnail). */
export function confirmTagProposal(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/confirm`, { method: 'POST' })
}

/** The `×` on a proposal: `state: 'proposed' → 'rejected'`, permanent —
 * unlike `removeConfirmedTag`, this proposal was never confirmed: there
 * is nothing to "remove", only to reject before it becomes a real
 * tag. */
export function rejectTagProposal(tagId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/assets/${assetId}/reject`, { method: 'POST' })
}

/** A pending AI proposal, enriched server-side with the tag name and
 * filename — the queue doesn't need a second round-trip per row. **Not**
 * grouped by tag by the backend (`GET /tags/proposals` returns a flat
 * list ordered by descending score, no `tag_id` grouping): grouping by
 * tag on the page is client-side, in `ReviewView.vue`. */
export interface Proposal {
  asset_id: string
  tag_id: string
  tag_name: string
  score?: number
  filename: string
  taken_at_utc?: string
}

export function fetchTagProposals(): Promise<Proposal[]> {
  return apiFetch('/api/v1/tags/proposals')
}

/** "Confirm all" (per group): confirms every pending proposal for that
 * tag in a single request, rather than one `confirmTagProposal` call per
 * row. */
export function confirmAllTagProposals(tagId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/proposals/confirm`, { method: 'POST' })
}

/** "Reject all" — mirror of `confirmAllTagProposals`. */
export function rejectAllTagProposals(tagId: string): Promise<null> {
  return apiFetch(`/api/v1/tags/${tagId}/proposals/reject`, { method: 'POST' })
}

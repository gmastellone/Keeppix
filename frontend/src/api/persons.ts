import { apiFetch } from './client'

export interface Person {
  id: string
  name: string | null
  hidden: boolean
  face_count: number | null
  /** Id of the face chosen as the cover — absent (not `null`) if never
   * set, matching `PersonView.cover_face_id` on the backend (`Option`,
   * `skip_serializing_if`). Only compared to highlight the "current" one
   * in `ChooseCoverDialog.vue` — the actual displayable cover photo is
   * `cover_hash`/`cover_thumbhash` below, not resolvable from this id
   * alone. */
  cover_face_id?: string
  /** A representative photo for this person's card — `GET /persons`
   * only (absent everywhere else `Person` is used), computed server-side
   * in the same query as `face_count`. Both absent only if the person
   * somehow has zero visible confirmed faces (shouldn't happen: a person
   * with none isn't visible at all — see the backend's own visibility
   * rule — but the type stays honest about it since nothing enforces
   * that invariant at this layer). */
  cover_hash?: string
  cover_thumbhash?: string
}

/** `include_hidden` is deliberately left out for the default caller
 * (never passed there: it only wants "non-hidden people with at least
 * one photo", the count filter still being the caller's job). The
 * People grid passes `true` exactly once, only to count how many are
 * hidden — never to show them. */
export function fetchPersons(includeHidden = false): Promise<Person[]> {
  return apiFetch(`/api/v1/persons${includeHidden ? '?include_hidden=true' : ''}`)
}

/** `POST /persons` — used by the lightbox's person picker ("Correct
 * person…") to create a person by typing a name: the face must be
 * assigned **afterward**, with a separate call to `faces.ts#assignFace`
 * (backend comment on `faces::assign`: "the client creates the person
 * first, then assigns the face to it"). Empty name → a nameless
 * person. */
export function createPerson(name: string): Promise<Person> {
  return apiFetch('/api/v1/persons', {
    method: 'POST',
    body: JSON.stringify({ name })
  })
}

/** Person detail: `GET /persons/{id}` **does not** carry `face_count`
 * (`PersonView.face_count` is an `Option`, populated only by the list
 * endpoint — backend comment on `PersonView`,
 * `crates/keeppix-api/src/routes/persons.rs:24-29`, "avoid paying for a
 * second query round-trip just for the count"). The detail view instead
 * computes its own count from the photos returned by
 * `runSearch({op:'person'})` — a different number only if a person has
 * more than one confirmed face in the same photo, a rare case, never the
 * reverse. */
export function fetchPerson(id: string): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}`)
}

/** `PATCH /persons/{id}` — a single route for three actions: rename,
 * hide/show, cover. Each field is independent:
 * `patchPerson(id, {hidden:true})` does not touch the name or cover. An
 * empty string for `name` clears the name (reverts to "unnamed"),
 * consistent with `PatchPersonRequest` — no frontend guard against
 * empty values for a person, unlike the equivalent guard for groups
 * (which bail out on an empty name via `if(!name) return`). */
export interface PersonPatchPayload {
  name?: string
  hidden?: boolean
  cover_face_id?: string
}

export function patchPerson(id: string, payload: PersonPatchPayload): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(payload)
  })
}

/** "Separate person": `POST /persons/{id}/separate` moves the given
 * `faceIds` to a new person (name optional, `''` if unspecified —
 * `PersonRepo::separate`, `crates/keeppix-db/src/persons.rs:326-392`).
 * **Not reversible from the UI** beyond a fresh manual merge (backend
 * comment: "the user should not expect an undo"). */
export function separatePerson(id: string, faceIds: string[], name: string): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}/separate`, {
    method: 'POST',
    body: JSON.stringify({ face_ids: faceIds, name })
  })
}

/** "Merge people": `POST /persons/{id}/merge` moves **all** faces of
 * `absorbed` onto the surviving `id` and deletes the absorbed people
 * (`PersonRepo::merge`, `crates/keeppix-db/src/persons.rs:278-324`) —
 * not reversible. */
export function mergePersons(id: string, absorbed: string[]): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}/merge`, {
    method: 'POST',
    body: JSON.stringify({ absorbed })
  })
}

/** Person groups — `crates/keeppix-api/src/routes/persons.rs:333-550`.
 * **The backend allows a person to belong to multiple groups**
 * (`person_group_members` is a many-to-many table, no uniqueness
 * constraint). `PeopleView.vue` enforces "at most one group per person"
 * client-side instead (removes the old membership before adding the new
 * one) — there is no real multi-group UI exposed here, even though the
 * backend would allow it. */
export interface PersonGroup {
  id: string
  name: string
  created_by: string
  created_at: string
}

export function fetchPersonGroups(): Promise<PersonGroup[]> {
  return apiFetch('/api/v1/person-groups')
}

export function createPersonGroup(name: string): Promise<PersonGroup> {
  return apiFetch('/api/v1/person-groups', {
    method: 'POST',
    body: JSON.stringify({ name })
  })
}

export function renamePersonGroup(id: string, name: string): Promise<PersonGroup> {
  return apiFetch(`/api/v1/person-groups/${id}`, {
    method: 'PATCH',
    body: JSON.stringify({ name })
  })
}

export function deletePersonGroup(id: string): Promise<null> {
  return apiFetch(`/api/v1/person-groups/${id}`, { method: 'DELETE' })
}

/** Ids of the people in the group — not a full `PersonView`: the grid
 * cross-references these ids with the list already loaded from
 * `fetchPersons()`, saving one network round-trip per group. */
export function fetchGroupMembers(groupId: string): Promise<string[]> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members`)
}

export function addGroupMember(groupId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members/${personId}`, { method: 'POST' })
}

export function removeGroupMember(groupId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members/${personId}`, { method: 'DELETE' })
}

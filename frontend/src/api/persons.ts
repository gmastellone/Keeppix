import { apiFetch } from './client'

// Fase 11 Task 7 (SP-3 §11, dimensione "Persone"): primo consumatore
// frontend di questa rotta — costruita in Fase 8, mai chiamata da questa
// app finora.
export interface Person {
  id: string
  name: string | null
  hidden: boolean
  face_count: number | null
}

/** `include_hidden` di proposito assente per il chiamante SP-3 §11 (mai
 * passato lì: vuole solo "persone non nascoste con almeno una foto", il
 * filtro sul conteggio resta comunque a carico del chiamante). Task 16
 * (1/N): la griglia Persone lo passa `true` una sola volta, solo per
 * contare quante sono nascoste (riga finale §31.2) — mai per mostrarle. */
export function fetchPersons(includeHidden = false): Promise<Person[]> {
  return apiFetch(`/api/v1/persons${includeHidden ? '?include_hidden=true' : ''}`)
}

/** `POST /persons` — usata dal selettore di persona del lightbox (§19.3,
 * "Correggi persona…") per creare una persona digitando un nome: il volto
 * va assegnato **dopo**, con una seconda chiamata a `faces.ts#assignFace`
 * (commento del backend su `faces::assign`: "il client crea prima la
 * persona, poi assegna il volto a quella"). Nome vuoto → persona senza
 * nome. */
export function createPerson(name: string): Promise<Person> {
  return apiFetch('/api/v1/persons', {
    method: 'POST',
    body: JSON.stringify({ name })
  })
}

/** Fase 11 Task 16 (1/N), §32 — dettaglio di una persona: `GET /persons/
 * {id}` **non** porta `face_count` (`PersonView.face_count` è `Option`,
 * valorizzato solo dall'elenco — commento del backend su `PersonView`,
 * `crates/keeppix-api/src/routes/persons.rs:24-29`, "non pagare un
 * secondo giro di query per il conteggio"). Il dettaglio calcola invece
 * il proprio conteggio dalle foto restituite da `runSearch({op:'person'})`
 * — un numero diverso solo se una persona ha più di un volto confermato
 * nella stessa foto, caso raro, mai il contrario. */
export function fetchPerson(id: string): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}`)
}

/** `PATCH /persons/{id}` — un'unica rotta per tre azioni del documento
 * (§32.3, controlli 2/6, e §33): rinomina, nascondi/mostra, copertina.
 * Ogni campo è indipendente: `patchPerson(id, {hidden:true})` non tocca
 * nome o copertina. Stringa vuota su `name` cancella il nome (torna
 * "senza nome"), coerente con `PatchPersonRequest` (§32.3 nota: "il nome
 * vuoto viene comunque assegnato, senza il controllo `if(!name) return`
 * che protegge invece i gruppi" — qui riprodotto: nessun controllo lato
 * frontend sul vuoto per la persona, a differenza del gruppo). */
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

/** Fase 11 Task 16 (2/N), §35 "Unisci persone": `POST /persons/{id}/
 * merge` sposta **tutti** i volti di `absorbed` sul sopravvissuto `id` e
 * cancella le persone assorbite (`PersonRepo::merge`,
 * `crates/keeppix-db/src/persons.rs:278-324`) — non reversibile, stesso
 * comportamento del documento. */
export function mergePersons(id: string, absorbed: string[]): Promise<Person> {
  return apiFetch(`/api/v1/persons/${id}/merge`, {
    method: 'POST',
    body: JSON.stringify({ absorbed })
  })
}

/** Gruppi di persone (§31.2-§31.3, §34) — `crates/keeppix-api/src/
 * routes/persons.rs:333-550`. **Il backend permette a una persona di
 * stare in più gruppi** (`person_group_members` è una tabella
 * molti-a-molti, nessun vincolo di unicità) — il documento invece
 * modella "una persona sta in al massimo un gruppo" (`groupId` singolo).
 * Non è un'invenzione da riprodurre alla cieca: `PeopleView.vue`
 * applica quel vincolo lato client (rimuove la vecchia appartenenza
 * prima di aggiungere la nuova), mai esposta qui una vera UI
 * multi-gruppo — il backend lo permetterebbe, il documento no. */
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

/** Id delle persone nel gruppo — non un `PersonView` completo: la
 * griglia incrocia questi id con l'elenco già caricato da
 * `fetchPersons()`, un giro di rete in meno per gruppo. */
export function fetchGroupMembers(groupId: string): Promise<string[]> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members`)
}

export function addGroupMember(groupId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members/${personId}`, { method: 'POST' })
}

export function removeGroupMember(groupId: string, personId: string): Promise<null> {
  return apiFetch(`/api/v1/person-groups/${groupId}/members/${personId}`, { method: 'DELETE' })
}

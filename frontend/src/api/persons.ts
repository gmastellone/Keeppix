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

/** `include_hidden` di proposito sempre assente (mai `true`): SP-3 §11
 * vuole solo "persone non nascoste con almeno una foto" — il filtro sul
 * conteggio va comunque applicato dal chiamante, la rotta non lo fa. */
export function fetchPersons(): Promise<Person[]> {
  return apiFetch('/api/v1/persons')
}

import { apiFetch } from './client'

/** Rispecchia `UserPreferences`/`UserPreferencesView` (`crates/keeppix-db/
 * src/preferences.rs`, `crates/keeppix-api/src/routes/preferences.rs`) —
 * un documento JSON per utente (`users.preferences`), mai consumato dal
 * frontend fino al Task 14: `GET/PATCH /users/me/preferences` esistono
 * dalla Fase 10 Task 9, con validazione server-side completa (`theme`
 * dev'essere uno dei tre valori, `grid_density` clampata 2-12/2-6,
 * `language` "it"/"en") — il client non deve rivalidare, solo rispecchiare
 * i tipi. */
export type Theme = 'chiaro' | 'scuro' | 'sistema'
export type Language = 'it' | 'en'

export interface GridDensityPreference {
  desktop: number
  mobile: number
}

export interface NotificationPreferences {
  digest: boolean
  condivisioni: boolean
  problemi: boolean
}

export interface UserPreferences {
  theme: Theme
  grid_density: GridDensityPreference
  notifications: NotificationPreferences
  language: Language
}

export function fetchPreferences(): Promise<UserPreferences> {
  return apiFetch('/api/v1/users/me/preferences')
}

/** Corpo di `PATCH` — a differenza di `UserPreferences`, i campi annidati
 * sono a loro volta parziali: il merge lato server è ricorsivo un livello
 * (`crates/keeppix-db/src/preferences.rs::apply_grid_patch`/
 * `apply_notifications_patch`), quindi si può scrivere solo
 * `grid_density.desktop` senza dover rileggere/reinviare anche `.mobile`. */
export interface UserPreferencesPatch {
  theme?: Theme
  grid_density?: Partial<GridDensityPreference>
  notifications?: Partial<NotificationPreferences>
  language?: Language
}

/** Merge parziale (§60.2, "Cosa scrive"): solo i campi passati vengono
 * aggiornati — `PATCH` lato server fa il merge, non serve rileggere prima
 * di scrivere un solo campo. */
export function patchPreferences(patch: UserPreferencesPatch): Promise<UserPreferences> {
  return apiFetch('/api/v1/users/me/preferences', {
    method: 'PATCH',
    body: JSON.stringify(patch)
  })
}

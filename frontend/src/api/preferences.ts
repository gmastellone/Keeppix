import { apiFetch } from './client'

/** Mirrors `UserPreferences`/`UserPreferencesView` (`crates/keeppix-db/
 * src/preferences.rs`, `crates/keeppix-api/src/routes/preferences.rs`) —
 * a per-user JSON document (`users.preferences`). `GET/PATCH
 * /users/me/preferences` do full server-side validation (`theme` must be
 * one of three values, `grid_density` clamped 2-12/2-6, `language`
 * "it"/"en") — the client should not revalidate, only mirror the
 * types. */
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

/** `PATCH` body — unlike `UserPreferences`, the nested fields are
 * themselves partial: the server-side merge is recursive one level deep
 * (`crates/keeppix-db/src/preferences.rs::apply_grid_patch`/
 * `apply_notifications_patch`), so `grid_density.desktop` can be written
 * alone without having to re-read/re-send `.mobile` too. */
export interface UserPreferencesPatch {
  theme?: Theme
  grid_density?: Partial<GridDensityPreference>
  notifications?: Partial<NotificationPreferences>
  language?: Language
}

/** Partial merge: only the fields passed in get updated — the
 * server-side `PATCH` does the merge, no need to read first before
 * writing a single field. */
export function patchPreferences(patch: UserPreferencesPatch): Promise<UserPreferences> {
  return apiFetch('/api/v1/users/me/preferences', {
    method: 'PATCH',
    body: JSON.stringify(patch)
  })
}

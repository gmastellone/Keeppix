import { apiFetch } from './client'
import type { User } from './auth'
import type { FolderView } from './folders'

export interface LibraryStorageView {
  free_bytes: number
  total_bytes: number
}

/** Culling badge: always present, even at 0. It's 0 until culling lots
 * are exposed by a real route — the backend states this explicitly in
 * its own comment (`BadgeCountsView.culling`, "Zero until lots exist on
 * the backend"), not a fake value on the frontend side. */
export interface BadgeCountsView {
  culling: number
  /** Pending tag + face proposals (`AssetTagRepo`/`FaceRepo`
   * `count_proposed_visible`, already safe if face recognition is off)
   * — the "Review" badge, shown only if > 0. */
  revision: number
}

export interface BootstrapResponse {
  user: User
  folders: FolderView[]
  /** Keyed by library id — same payload as `GET /libraries/{id}/storage`. */
  storage: Record<string, LibraryStorageView>
  badges: BadgeCountsView
}

/**
 * A single call for the shell's data (desktop sidebar + mobile
 * header/"More"): folders, free space per library, navigation badges.
 * Distinct from `session.bootstrap()` (setup + authentication state,
 * `stores/session.ts`) despite the identical name — two different
 * concepts that the backend happens to call "bootstrap" both times.
 */
export function fetchBootstrap(): Promise<BootstrapResponse> {
  return apiFetch('/api/v1/bootstrap')
}

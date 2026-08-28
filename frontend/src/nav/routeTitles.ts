// Extracted from AppTopbar.vue to avoid a second, diverging copy in the
// mobile header (`mobileTitleFor()`): same map, the exact same text per
// route — the topbar shows the breadcrumb, the mobile header shows the
// title, but for every route covered today they're the exact same string.
import { ref } from 'vue'

export const ROUTE_TITLE_KEYS: Record<string, string> = {
  '/': 'topbar.allPhotos',
  '/favorites': 'favorites.title',
  '/settings': 'settings.title',
  '/profile': 'profile.title',
  '/tags': 'tags.title',
  '/review': 'review.title',
  '/persons': 'persons.title',
  '/search': 'nav.cerca',
  '/culling': 'culling.entry',
  '/map': 'maps.entry',
  '/shares': 'shares.entry',
  '/albums': 'albums.entry',
  '/albums/new': 'albums.createButton',
  '/trash': 'trash.entry',
  '/problems': 'problems.title',
  '/duplicates': 'duplicates.entry',
  '/batch-edit': 'batchEdit.title',
  '/folders': 'folders.title',
  '/users': 'users.title',
  '/groups': 'groups.title'
}

/** Name of the album open at `/albums/:id`. The first dynamic route with an
 * "open item" that's observable from outside the view itself —
 * `AlbumDetailView` writes it on load and clears it on unmount. A shared
 * module-level ref rather than a single-field Pinia store, same approach as
 * `useDensity` (a second consumer, `AppMobileHeader`, showed up right
 * away). */
export const activeAlbumName = ref<string | null>(null)

/** Same pattern for `/persons/:id` — the person's name, not a placeholder
 * like "Unnamed person": when there's no name the breadcrumb just shows
 * "People" (no second segment), leaving `PersonDetailView` to write the
 * real name or `null` here. */
export const activePersonName = ref<string | null>(null)

/** Same pattern for `/culling/:lotId`: `CullingLotView` writes the lot's
 * name here on load and clears it on unmount. */
export const activeCullingLotName = ref<string | null>(null)

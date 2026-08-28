import { onBeforeUnmount, onMounted, ref } from 'vue'

import { fetchPreferences, patchPreferences } from '@/api/preferences'
import { clampDensity } from '@/timeline/justify'

// Extracted from `TimelineView.vue` when a second consumer appeared
// (`FavoritesView` — same justified grid, same density).
//
// Two distinct values — desktop and mobile, each with its own range
// (`clampDensity`) — instead of a single global toggle. Persistence moved
// from `localStorage` to `GET/PATCH /users/me/preferences`: a value per
// user, not per browser, matching the requirement that density be "saved
// separately for desktop and mobile" — meaning two devices on the same
// account, not two tabs in the same browser.
//
// Sync happens "on next visit", not as a value shared live across already
// mounted views: each call to `useDensity()` starts from a default and
// reconciles it asynchronously with the server on mount — a write via
// `setDensity` does not notify other instances already open elsewhere.
const MOBILE_BREAKPOINT_QUERY = '(max-width: 767px)'

export function useDensity() {
  const isMobile = ref(window.matchMedia(MOBILE_BREAKPOINT_QUERY).matches)
  const density = ref(clampDensity(isMobile.value ? 3 : 4, isMobile.value))
  let query: MediaQueryList | undefined

  function syncDevice() {
    if (query) isMobile.value = query.matches
  }

  onMounted(() => {
    query = window.matchMedia(MOBILE_BREAKPOINT_QUERY)
    syncDevice()
    query.addEventListener('change', syncDevice)

    void fetchPreferences()
      .then((prefs) => {
        const stored = isMobile.value ? prefs.grid_density.mobile : prefs.grid_density.desktop
        density.value = clampDensity(stored, isMobile.value)
      })
      .catch(() => {
        // No preference could be read (network issue, session just
        // expired): keep the default already assigned above (4 desktop, 3
        // mobile).
      })
  })

  onBeforeUnmount(() => {
    query?.removeEventListener('change', syncDevice)
  })

  function setDensity(next: number) {
    density.value = clampDensity(next, isMobile.value)
    const patch = isMobile.value ? { mobile: density.value } : { desktop: density.value }
    void patchPreferences({ grid_density: patch }).catch(() => {})
  }

  return { density, setDensity, isMobile }
}

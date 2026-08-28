// Theme is set from Settings -> Appearance, a single place instead of two
// redundant controls — there's no quick-toggle elsewhere in the UI, by
// construction (no second consumer calls it).
//
// A Pinia store, not a module-ref composable like `useDensity`: theme is
// genuinely global — it ALWAYS changes the entire interface at the same
// instant, not just the view that reads it — and this file is the only
// place that writes `data-theme` on the document, so it needs a shared
// reactive singleton, not a fresh value per caller like density (where
// "synced on next visit" is already the accepted behavior).
//
// `--duration-theme` (design/tokens.ts) was already labeled "theme
// change, 2 cases" before this feature existed — it was anticipated in
// the design tokens, not a surprise addition.
//
// Theme is read from the **server-side** preferences
// (`GET /users/me/preferences`, never consumed by the frontend before
// this) — which requires an authenticated session: before login the page
// keeps the existing default behavior (`@media (prefers-color-scheme:
// dark)` in `style.css`, unchanged). This isn't a stylistic choice: there
// is no route that reads an unauthenticated user's preferences.
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { fetchPreferences, patchPreferences, type Theme } from '@/api/preferences'

export const useThemeStore = defineStore('theme', () => {
  const preference = ref<Theme>('chiaro')
  const loaded = ref(false)
  let systemQuery: MediaQueryList | undefined

  function resolveDataTheme(): 'light' | 'dark' {
    if (preference.value === 'scuro') return 'dark'
    if (preference.value === 'chiaro') return 'light'
    return systemQuery?.matches ? 'dark' : 'light'
  }

  function apply() {
    document.documentElement.setAttribute('data-theme', resolveDataTheme())
  }

  function onSystemChange() {
    if (preference.value === 'sistema') apply()
  }

  async function load() {
    if (!systemQuery) {
      systemQuery = window.matchMedia('(prefers-color-scheme: dark)')
      systemQuery.addEventListener('change', onSystemChange)
    }
    try {
      const prefs = await fetchPreferences()
      preference.value = prefs.theme
    } catch {
      // No preference could be read (network error, session just
      // expired): stays at the light default.
    }
    apply()
    loaded.value = true
  }

  /** Restores the default behavior (follow the system) when there's no
   * longer a user whose preferences apply — e.g. after logout, before
   * returning to the login page. */
  function reset() {
    preference.value = 'chiaro'
    loaded.value = false
    document.documentElement.removeAttribute('data-theme')
  }

  async function setPreference(next: Theme) {
    const previous = preference.value
    preference.value = next
    apply()
    try {
      await patchPreferences({ theme: next })
    } catch {
      preference.value = previous
      apply()
      throw new Error('theme-patch-failed')
    }
  }

  return { preference, loaded, load, reset, setPreference }
})

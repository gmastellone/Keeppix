import { onBeforeUnmount, onMounted, ref } from 'vue'

import { fetchPreferences, patchPreferences } from '@/api/preferences'
import { clampDensity } from '@/timeline/justify'

// Estratto da `TimelineView.vue` (Fase 11 Task 4) al comparire del
// secondo consumatore (Task 7: `FavoritesView` — stessa griglia
// giustificata, stessa densità). Stesso principio già seguito per
// `useIsMobile.ts`/`nav/routeTitles.ts`: dedup proattivo appena serve
// altrove, non un refactor speculativo.
//
// Task 14 (1/N), §60.2 "Densità griglia": due valori distinti — desktop
// e mobile, ognuno con il proprio intervallo (`clampDensity`, Task 14) —
// invece dell'unico interruttore globale di prima. Persistenza spostata
// da `localStorage` a `GET/PATCH /users/me/preferences` (Fase 10 Task 9,
// mai consumate dal frontend prima di questo task): un valore per utente,
// non per browser, coerente con "salvata separatamente da desktop e
// mobile" del documento — che parla di due dispositivi dello stesso
// account, non di due schede dello stesso browser.
//
// Stesso principio di sincronizzazione "alla prossima visita" di prima
// (non un valore condiviso in tempo reale fra viste già montate): ogni
// chiamata a `useDensity()` parte da un valore predefinito e lo
// riconcilia in modo asincrono col server al montaggio — chi scrive con
// `setDensity` non notifica le altre istanze già aperte altrove.
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
        // Nessuna preferenza leggibile (rete, sessione appena scaduta):
        // resta il predefinito già assegnato sopra (§60.2: 4 desktop, 3
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

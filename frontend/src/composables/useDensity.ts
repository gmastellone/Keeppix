import { ref } from 'vue'

import { clampDensity } from '@/timeline/justify'

// Estratto da `TimelineView.vue` (Fase 11 Task 4) al comparire del
// secondo consumatore (Task 7: `FavoritesView` — stessa griglia
// giustificata, stessa densità). Stesso principio già seguito per
// `useIsMobile.ts`/`nav/routeTitles.ts`: dedup proattivo appena serve
// altrove, non un refactor speculativo.
//
// Il documento funzionale (riga 1745) mette la densità in Impostazioni
// (Task 14, non ancora costruito), non in un controllo di vista — un
// unico interruttore globale, non uno per vista. La stessa chiave di
// `localStorage` qui, condivisa fra Timeline e Preferiti, è ciò che li fa
// restare sincronizzati: cambiarla in un posto la cambia anche nell'altro
// alla prossima visita.
const DENSITY_KEY = 'keeppix.density'

export function useDensity() {
  const density = ref(clampDensity(Number(localStorage.getItem(DENSITY_KEY) ?? 6)))

  function setDensity(next: number) {
    density.value = clampDensity(next)
    localStorage.setItem(DENSITY_KEY, String(density.value))
  }

  return { density, setDensity }
}

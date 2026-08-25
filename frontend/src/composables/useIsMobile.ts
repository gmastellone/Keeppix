import { onBeforeUnmount, onMounted, ref } from 'vue'

// Estratto da `AppShell.vue` (Fase 11 Task 6) al comparire del secondo
// consumatore (Task 7: `PhotoTile` — "sapere se siamo su mobile è compito
// di AppShell, non di questo componente", commento già presente su
// `enableLongPress` prima ancora che questo file esistesse). Stesso
// principio già seguito per `nav/routeTitles.ts`: dedup proattivo appena
// serve altrove, non un refactor speculativo.
//
// Nota vincolante del piano, invariata dal Task 6: "commuta per larghezza,
// non per interruttore" — un vero media query, non uno stato manuale come
// `state.device` nel prototipo. Nessuna soglia numerica esiste nel
// documento funzionale né nel mockup: 768px resta il breakpoint `md` di
// Tailwind, non un valore misurato sul prototipo — debito dichiarato, non
// assunto in silenzio.
const MOBILE_BREAKPOINT_QUERY = '(max-width: 767px)'

export function useIsMobile() {
  const isMobile = ref(false)
  let query: MediaQueryList | undefined

  function sync() {
    if (query) isMobile.value = query.matches
  }

  onMounted(() => {
    query = window.matchMedia(MOBILE_BREAKPOINT_QUERY)
    sync()
    query.addEventListener('change', sync)
  })

  onBeforeUnmount(() => {
    query?.removeEventListener('change', sync)
  })

  return { isMobile }
}

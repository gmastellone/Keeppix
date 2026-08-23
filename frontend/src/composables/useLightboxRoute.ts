import { ref, watch } from 'vue'
import type { Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

// Fase 11 Task 3 (router): il visore a schermo intero come stato
// dell'URL, non un ref locale isolato. Il documento funzionale (§7)
// indica esplicitamente il caso che rende necessaria una rotta reale —
// "mando a un collega il link a questa foto" — e dichiara altrettanto
// esplicitamente che il prototipo non ce l'ha: *"niente
// history.pushState, niente URL per vista... il ripristino della
// posizione di scorrimento non è implementato"*. Non c'è quindi un
// comportamento del prototipo da riprodurre qui — è una scelta di
// design nuova, documentata di seguito invece che dedotta da un
// commento del mockup che non esiste.
//
// Compone qualunque vista con un visore a foto singola (Timeline,
// Cerca) dietro un'unica fonte di verità: il parametro di query
// `photo`. Tre casi distinti, tre azioni di navigazione diverse:
// - **Apertura** (tocco su una tessera): `push` — un Indietro del
//   browser deve chiudere il visore, non uscire dalla vista.
// - **Passo avanti/indietro** dentro il visore già aperto: `replace` —
//   scorrere venti foto non deve accumulare venti voci di cronologia
//   che l'utente dovrebbe poi attraversare a ritroso una per una.
// - **Chiusura**: `back()` se l'apertura è avvenuta in questa stessa
//   sessione di navigazione (il caso normale, il click su una
//   tessera); altrimenti `replace` senza `photo` — un link diretto o
//   un ricaricamento della pagina non hanno una voce nostra da
//   rimuovere, e usare `back()` lì porterebbe fuori dall'app.
export function useLightboxRoute<T extends { id: string }>(
  findLocal: (id: string) => T | undefined,
  loadRemote: (id: string) => Promise<T>
) {
  const route = useRoute()
  const router = useRouter()
  const viewing = ref<T | null>(null) as Ref<T | null>
  let openedViaPush = false

  watch(
    () => route.query.photo,
    async (photoId) => {
      if (typeof photoId !== 'string') {
        viewing.value = null
        return
      }
      if (viewing.value?.id === photoId) return
      viewing.value = findLocal(photoId) ?? (await loadRemote(photoId))
    },
    { immediate: true }
  )

  function open(asset: T) {
    openedViaPush = true
    return router.push({ query: { ...route.query, photo: asset.id } })
  }

  function openById(id: string) {
    return router.replace({ query: { ...route.query, photo: id } })
  }

  function step(next: T | undefined) {
    if (!next) return Promise.resolve()
    return router.replace({ query: { ...route.query, photo: next.id } })
  }

  function close() {
    if (openedViaPush) {
      openedViaPush = false
      return router.back()
    }
    const query = { ...route.query }
    delete query.photo
    return router.replace({ query })
  }

  return { viewing, open, openById, step, close }
}

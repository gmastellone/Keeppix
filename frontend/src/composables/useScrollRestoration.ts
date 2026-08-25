import { onBeforeUnmount, onMounted } from 'vue'
import type { Ref } from 'vue'
import { useRoute } from 'vue-router'

// Fase 11 Task 3: "il ripristino della posizione di scorrimento non è
// implementato: tornando in una sezione si riparte dall'alto" (documento
// funzionale §7.6) — dichiarato esplicitamente assente nel prototipo, una
// scelta di design nuova qui, non una riproduzione.
//
// `scrollBehavior` di vue-router (già impostato in router.ts) copre solo lo
// scroll di `window`, ma questa app non ne ha uno: `html, body, #app` sono
// vincolati a `height:100%` (style.css), ogni vista scorre dentro un
// proprio contenitore interno (`overflow-auto`). Nessuna vista è tenuta
// viva fra una navigazione e l'altra (niente `<KeepAlive>` nel router), quindi
// l'elemento scrollabile è un nodo DOM nuovo ogni volta — serve una cache
// esplicita per rotta, non un semplice "ricordati dov'eri" sull'istanza.
const positions = new Map<string, number>()

export function useScrollRestoration(el: Ref<HTMLElement | null>, key?: string) {
  const route = useRoute()
  // Catturata una volta sola al montaggio: `route.fullPath` a `onUnmounted`
  // potrebbe già riflettere la rotta di arrivo, non quella che si sta
  // lasciando, a seconda dell'ordine con cui Vue Router aggiorna route e
  // albero dei componenti.
  const cacheKey = key ?? route.fullPath

  function save() {
    if (el.value) positions.set(cacheKey, el.value.scrollTop)
  }

  onMounted(() => {
    const saved = positions.get(cacheKey)
    if (saved !== undefined && el.value) {
      el.value.scrollTop = saved
    }
  })

  // `onUnmounted`, non `onBeforeUnmount`, arriverebbe tardi: Vue azzera i
  // ref del template prima di invocare gli hook di unmount, quindi
  // `el.value` sarebbe già `null` e non ci sarebbe nulla da salvare.
  onBeforeUnmount(save)

  return { save }
}

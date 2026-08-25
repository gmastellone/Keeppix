import { defineStore } from 'pinia'
import { ref } from 'vue'

// SP-2 (Fase 11 Task 2): due pool di selezione paralleli e indipendenti —
// la libreria (Timeline/Preferiti/Cerca/Album/Persona, documento
// funzionale §12.8) e il lotto di culling, che ha i propri comandi e non
// Album/Elimina. Nota vincolante del piano: "non si parlano e non si
// azzerano a vicenda". Non un solo pool con un flag di contesto: due
// closure separate, così toccare l'una non può mai, per costruzione,
// toccare l'altra — verificato da un test che tocca l'una e controlla
// che l'altra resti intonsa, non solo che esistano due proprietà.
function createSelectionPool() {
  const selectedIds = ref(new Set<string>())

  function toggle(id: string) {
    const next = new Set(selectedIds.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selectedIds.value = next
  }

  function clear() {
    selectedIds.value = new Set()
  }

  /** "Seleziona tutte" (documento funzionale §12.3): toggle di gruppo sul
   * visibile — se tutto ciò che è passato è già selezionato lo toglie,
   * altrimenti lo aggiunge **senza** rimuovere foto già selezionate
   * altrove. L'etichetta non cambia mai, anche quando in quel momento
   * deseleziona (§12.7) — non è compito di questa funzione, resta un
   * dettaglio del componente di resa. */
  function selectAllVisible(visibleIds: string[]) {
    const allSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedIds.value.has(id))
    const next = new Set(selectedIds.value)
    if (allSelected) {
      visibleIds.forEach((id) => next.delete(id))
    } else {
      visibleIds.forEach((id) => next.add(id))
    }
    selectedIds.value = next
  }

  return { selectedIds, toggle, clear, selectAllVisible }
}

export const useSelectionStore = defineStore('selection', () => {
  const library = createSelectionPool()
  const culling = createSelectionPool()

  return { library, culling }
})

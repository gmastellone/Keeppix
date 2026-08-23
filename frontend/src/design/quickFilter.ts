// SP-3 (Fase 11 Task 2): la logica di combinazione del filtro rapido a
// chip (documento funzionale §11.3, `photoMatchesBrowseFilters`) — pura
// e indipendente da qualunque modello di foto/tag/persona reale, perché
// quegli store non esistono ancora in questa sessione. Regola esatta del
// documento: dentro una stessa dimensione i valori scelti sono in OR,
// fra dimensioni diverse è un AND (esempio del commento originale: "Tipo
// = RAW E Persone = Marta E Luogo = Urbino").

export type FilterSelection = Record<string, Set<string>>

export interface MatchDimension<T> {
  id: string
  /** I valori che *questo* elemento porta per questa dimensione — un
   * array anche per un campo a valore singolo (es. la fotocamera),
   * perché l'OR-dentro-la-dimensione si esprime allo stesso modo per
   * campi singoli e multipli (tag). Una dimensione disattivata (es.
   * "Persone" quando il riconoscimento volti è spento) può restituire
   * sempre `[]`: nessun valore potrà mai intersecare la selezione,
   * riproducendo lo `return false` secco del documento senza bisogno di
   * un caso speciale nel confronto. */
  getValues: (item: T) => string[]
}

export function activeFilterCount(selection: FilterSelection): number {
  return Object.values(selection).reduce((sum, set) => sum + set.size, 0)
}

export function matchesFilters<T>(item: T, dimensions: MatchDimension<T>[], selection: FilterSelection): boolean {
  return dimensions.every((dimension) => {
    const selected = selection[dimension.id]
    if (!selected || selected.size === 0) return true
    const itemValues = dimension.getValues(item)
    return itemValues.some((value) => selected.has(value))
  })
}

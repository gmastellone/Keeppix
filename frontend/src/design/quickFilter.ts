// The combination logic for the quick-filter chips (`photoMatchesBrowseFilters`)
// is pure and independent of any real photo/tag/person model, so it can be
// unit-tested on its own. Rule: within a dimension the chosen values are
// OR'd together; across dimensions it's an AND (e.g. "Type = RAW AND People
// = Marta AND Location = Urbino").

export type FilterSelection = Record<string, Set<string>>

export interface MatchDimension<T> {
  id: string
  /** The values *this* item carries for this dimension — an array even for
   * a single-value field (e.g. camera), because OR-within-a-dimension is
   * expressed the same way for both single and multi-value fields (tags).
   * A disabled dimension (e.g. "People" when face recognition is off) can
   * always return `[]`: no value will ever intersect the selection, which
   * naturally reproduces a hard `return false` without needing a special
   * case in the comparison. */
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

import { onUnmounted } from 'vue'

/**
 * Collapses a burst of rapid calls into one: `fn` fires only after
 * `delayMs` of quiet since the last call to the returned function, not once
 * per call. Meant for live-event handlers (`assets.upserted` over the
 * WebSocket fires once per finished background job — tens per second during
 * a large import) that would otherwise re-run an expensive, UI-disruptive
 * refresh far more often than anyone can act on.
 *
 * The pending timer is cleared on unmount, so a debounced call never fires
 * against a component that's already gone.
 */
export function useDebouncedCallback(fn: () => void, delayMs: number): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined

  onUnmounted(() => clearTimeout(timer))

  return () => {
    clearTimeout(timer)
    timer = setTimeout(fn, delayMs)
  }
}

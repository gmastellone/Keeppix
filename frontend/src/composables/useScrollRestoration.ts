import { onBeforeUnmount, onMounted } from 'vue'
import type { Ref } from 'vue'
import { useRoute } from 'vue-router'

// Scroll position restoration is a deliberate new design decision here, not
// a reproduction of prototype behavior — the prototype explicitly left it
// unimplemented ("returning to a section starts back at the top").
//
// vue-router's `scrollBehavior` (already set in router.ts) only covers
// `window` scrolling, but this app doesn't scroll the window: `html, body,
// #app` are constrained to `height:100%` (style.css), and every view
// scrolls inside its own inner container (`overflow-auto`). No view is kept
// alive across navigations (no `<KeepAlive>` in the router), so the
// scrollable element is a brand-new DOM node every time — this needs an
// explicit per-route cache, not a simple "remember where you were" on the
// instance.
const positions = new Map<string, number>()

export function useScrollRestoration(el: Ref<HTMLElement | null>, key?: string) {
  const route = useRoute()
  // Captured once on mount: `route.fullPath` read in `onUnmounted` could
  // already reflect the destination route rather than the one being left,
  // depending on the order in which Vue Router updates the route and the
  // component tree.
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

  // `onUnmounted`, not `onBeforeUnmount`, would fire too late: Vue clears
  // template refs before invoking unmount hooks, so `el.value` would
  // already be `null` and there'd be nothing to save.
  onBeforeUnmount(save)

  return { save }
}

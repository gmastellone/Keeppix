import { onBeforeUnmount, onMounted, ref } from 'vue'

// Extracted from `AppShell.vue` when a second consumer appeared
// (`PhotoTile` — knowing whether we're on mobile is AppShell's job, not
// this component's).
//
// Binding constraint: switch based on viewport width, not a manual toggle —
// a real media query, not manual state like `state.device` in the
// prototype. No numeric threshold exists in the functional spec or the
// mockup: 768px is just Tailwind's `md` breakpoint, not a value measured
// from the prototype — a known, deliberately flagged gap, not a silent
// assumption.
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

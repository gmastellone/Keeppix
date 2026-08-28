import { ref, watch } from 'vue'
import type { Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

// The full-screen lightbox as URL state, not an isolated local ref. This is
// a deliberate design decision (the case that makes a real route
// necessary is sharing a direct link to a photo with someone else), not a
// reproduction of prototype behavior — the prototype had no such thing.
//
// Composes any view with a single-photo lightbox (Timeline, Search) behind
// one source of truth: the `photo` query parameter. Three distinct cases,
// three different navigation actions:
// - **Opening** (tapping a tile): `push` — a browser Back should close the
//   lightbox, not leave the view.
// - **Stepping forward/back** inside an already-open lightbox: `replace` —
//   scrolling through twenty photos shouldn't pile up twenty history
//   entries the user would then have to step back through one by one.
// - **Closing**: `back()` if the lightbox was opened during this same
//   navigation session (the normal case, a tap on a tile); otherwise
//   `replace` without `photo` — a direct link or a page reload has no
//   entry of ours to remove, and using `back()` there would navigate out
//   of the app.
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

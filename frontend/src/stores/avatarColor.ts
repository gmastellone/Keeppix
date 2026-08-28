// No server-side preference exists for this field:
// `GET/PATCH /users/me/preferences` has no color field — adding one would
// need a new route/column, out of scope for a UI-only task.
//
// `localStorage`, not server-side preferences like `theme`/density:
// unlike those two values — which are also read by **other** users when
// sharing (never the case here: the hash-based `avatarColorFor(id)` is
// the only thing another user ever sees, NEVER this personal choice) —
// an avatar color choice is by construction visible only in one's own
// browser (sidebar/header/current user's Profile). Keyed by `userId`,
// not global: on a browser shared by multiple accounts on the instance,
// one user's choice must not appear under another user accessing from
// the same device — the same principle that moved grid density from
// `localStorage` to server-side preferences (a value per user, not per
// browser), applied here in the only way available without a new
// column: it doesn't follow the account across different devices, the
// one honest limitation that remains.
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

export interface AvatarColorOption {
  id: string
  label: string
  hex: string | null
}

export const AVATAR_COLOR_OPTIONS: AvatarColorOption[] = [
  { id: 'accent', label: 'Arancione (predefinito)', hex: null },
  { id: 'blu', label: 'Blu', hex: '#3B82C4' },
  { id: 'verde', label: 'Verde', hex: '#2E9E5B' },
  { id: 'viola', label: 'Viola', hex: '#8B5CF6' },
  { id: 'rosa', label: 'Rosa', hex: '#E0578A' },
  { id: 'teal', label: 'Verde acqua', hex: '#0E9488' },
  { id: 'grafite', label: 'Grafite', hex: '#3A3A3A' },
  { id: 'rosso', label: 'Rosso', hex: '#D9503F' }
]

const DEFAULT_ID = 'accent'

function storageKey(userId: string): string {
  return `keeppix.avatarColor.${userId}`
}

export const useAvatarColorStore = defineStore('avatarColor', () => {
  const colorId = ref<string>(DEFAULT_ID)

  const hex = computed(() => AVATAR_COLOR_OPTIONS.find((o) => o.id === colorId.value)?.hex ?? null)

  function load(userId: string) {
    try {
      const stored = localStorage.getItem(storageKey(userId))
      colorId.value = stored && AVATAR_COLOR_OPTIONS.some((o) => o.id === stored) ? stored : DEFAULT_ID
    } catch {
      colorId.value = DEFAULT_ID
    }
  }

  function setColor(userId: string, id: string) {
    colorId.value = id
    try {
      localStorage.setItem(storageKey(userId), id)
    } catch {
      // localStorage unavailable (private browsing, quota full): the
      // choice stays valid for this page session, it just won't survive
      // a reload — no error worth showing for a purely cosmetic
      // preference.
    }
  }

  function reset() {
    colorId.value = DEFAULT_ID
  }

  return { colorId, hex, load, setColor, reset }
})

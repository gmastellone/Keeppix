// Fase 11 Task 14 (2/N), §61.2 "Colore avatar" — documento funzionale
// verificato riga per riga (righe 9154-9180). Nessuna preferenza server per
// questo campo: `GET/PATCH /users/me/preferences` (Fase 10 Task 9) non ha un
// campo colore — costruirne uno richiederebbe una rotta/colonna nuova, fuori
// scope per un task di sola interfaccia (stesso principio seguito per ogni
// altra deviazione di questa fase).
//
// `localStorage`, non le preferenze server come `theme`/densità: a
// differenza di quei due valori — letti anche da **altri** utenti in
// condivisione (mai il caso qui: `avatarColorFor(id)` hash-based è l'unica
// cosa che un altro utente vede mai, MAI questa scelta personale) — una
// scelta di colore avatar è per costruzione visibile solo nel proprio
// browser (sidebar/header/Profilo dell'utente corrente). Chiave per
// `userId`, non globale: su un browser condiviso da più account
// dell'istanza, la scelta di un utente non deve comparire sotto un altro
// che accede dallo stesso dispositivo — lo stesso principio che ha spostato
// la densità della griglia da `localStorage` a preferenze server (un valore
// per utente, non per browser), applicato qui nel solo modo disponibile
// senza una colonna nuova: non segue l'account fra dispositivi diversi,
// l'unico limite onesto rimasto.
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
      // localStorage indisponibile (modalità privata, quota piena): la
      // scelta resta valida per questa sessione di pagina, solo non
      // sopravvive a un ricaricamento — nessun errore da mostrare per una
      // preferenza puramente cosmetica.
    }
  }

  function reset() {
    colorId.value = DEFAULT_ID
  }

  return { colorId, hex, load, setColor, reset }
})

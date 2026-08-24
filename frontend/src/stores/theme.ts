// Fase 11 Task 14 (1/N) — documento funzionale §60.1 "Aspetto", verificato
// contro il commento già presente in `AppTopbar.vue` (Task 6, 2/N): "il
// tema si imposta da Impostazioni → Aspetto, un solo posto invece di due
// controlli ridondanti" — nessun interruttore rapido altrove, per
// costruzione (nessun secondo consumatore lo chiama).
//
// Store Pinia, non un composable a ref di modulo come `useDensity`: il
// tema è overo davvero globale — cambia SEMPRE l'intera interfaccia nello
// stesso istante, non solo la vista che lo legge — e questo file è
// l'unico punto che scrive `data-theme` sul documento, quindi ha bisogno
// di un singleton reattivo condiviso, non di un valore fresco per ogni
// chiamante come la densità (dove "sincronizzato alla prossima visita" è
// già il comportamento accettato).
//
// `--duration-theme` (design/tokens.ts, già presente dal Task 1) era
// dichiarata "cambio di tema, 2 casi" da prima ancora che questa funzione
// esistesse — la feature era già anticipata nei token, non un'aggiunta a
// sorpresa.
//
// Il tema si legge dalle preferenze **del server**
// (`GET /users/me/preferences`, Fase 10 Task 9, mai consumate dal
// frontend prima d'ora) — richiede quindi una sessione autenticata:
// prima del login la pagina resta al comportamento di default già
// esistente (`@media (prefers-color-scheme: dark)` in `style.css`,
// invariato). Non è una scelta stilistica: non esiste alcuna rotta che
// legga le preferenze di un utente non autenticato.
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { fetchPreferences, patchPreferences, type Theme } from '@/api/preferences'

export const useThemeStore = defineStore('theme', () => {
  const preference = ref<Theme>('chiaro')
  const loaded = ref(false)
  let systemQuery: MediaQueryList | undefined

  function resolveDataTheme(): 'light' | 'dark' {
    if (preference.value === 'scuro') return 'dark'
    if (preference.value === 'chiaro') return 'light'
    return systemQuery?.matches ? 'dark' : 'light'
  }

  function apply() {
    document.documentElement.setAttribute('data-theme', resolveDataTheme())
  }

  function onSystemChange() {
    if (preference.value === 'sistema') apply()
  }

  async function load() {
    if (!systemQuery) {
      systemQuery = window.matchMedia('(prefers-color-scheme: dark)')
      systemQuery.addEventListener('change', onSystemChange)
    }
    try {
      const prefs = await fetchPreferences()
      preference.value = prefs.theme
    } catch {
      // Nessuna preferenza leggibile (rete, sessione appena scaduta): resta
      // "chiaro", lo stesso predefinito del documento (§60.2).
    }
    apply()
    loaded.value = true
  }

  /** Ripristina il comportamento di default (segue il sistema) quando non
   * c'è più un utente le cui preferenze applicare — es. dopo il logout,
   * prima di tornare alla pagina di login. */
  function reset() {
    preference.value = 'chiaro'
    loaded.value = false
    document.documentElement.removeAttribute('data-theme')
  }

  async function setPreference(next: Theme) {
    const previous = preference.value
    preference.value = next
    apply()
    try {
      await patchPreferences({ theme: next })
    } catch {
      preference.value = previous
      apply()
      throw new Error('theme-patch-failed')
    }
  }

  return { preference, loaded, load, reset, setPreference }
})

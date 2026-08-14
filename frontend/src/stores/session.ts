import { defineStore } from 'pinia'
import { ref } from 'vue'

import * as authApi from '@/api/auth'
import type { SetupPayload, User } from '@/api/auth'
import { ApiProblem } from '@/api/client'

export type { User }

export const useSessionStore = defineStore('session', () => {
  const user = ref<User | null>(null)
  const initialised = ref<boolean | null>(null)
  const ready = ref(false)
  /** True se l'ultimo logout ha azzerato la sessione localmente senza
   * conferma dal server. Letto (e azzerato) dalla vista che accoglie
   * l'utente dopo il redirect, per segnalarlo senza bloccare l'uscita. */
  const logoutError = ref(false)

  /** Determina lo stato dell'istanza e ripristina la sessione se presente. */
  async function bootstrap(): Promise<void> {
    const status = await authApi.getSetupStatus()
    initialised.value = status.initialised

    if (status.initialised) {
      try {
        const result = await authApi.me()
        user.value = result.user
      } catch (error) {
        // 401 è normale: nessuna sessione attiva.
        if (!(error instanceof ApiProblem) || error.status !== 401) throw error
        user.value = null
      }
    }
    ready.value = true
  }

  async function login(username: string, password: string): Promise<void> {
    const result = await authApi.login(username, password)
    user.value = result.user
  }

  async function setup(payload: SetupPayload): Promise<void> {
    const result = await authApi.setupAccount(payload)
    user.value = result.user
    initialised.value = true
  }

  /**
   * Il logout è un'azione di sicurezza: se la revoca server-side fallisce
   * (quasi certamente un errore di rete — il backend risponde comunque
   * `204` sui fallimenti che riesce a gestire), l'utente non deve restare
   * bloccato in un'interfaccia che lo mostra ancora autenticato. Si azzera
   * lo stato locale in ogni caso, e si segnala l'accaduto tramite
   * `logoutError` invece di propagare l'eccezione al chiamante.
   */
  async function logout(): Promise<void> {
    try {
      await authApi.logout()
      logoutError.value = false
    } catch {
      logoutError.value = true
    } finally {
      user.value = null
    }
  }

  return { user, initialised, ready, logoutError, bootstrap, login, setup, logout }
})

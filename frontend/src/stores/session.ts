import { defineStore } from 'pinia'
import { ref } from 'vue'

import * as authApi from '@/api/auth'
import type { SetupPayload, User } from '@/api/auth'
import { ApiProblem } from '@/api/client'

export type { User }

/** TTL cookie 30 giorni: un giro ogni 12 ore, e solo a scheda visibile,
 * tiene la sessione viva senza ruotare tutta la notte su un Pi. */
export const SESSION_REFRESH_INTERVAL_MS = 12 * 60 * 60 * 1000

export const useSessionStore = defineStore('session', () => {
  const user = ref<User | null>(null)
  const initialised = ref<boolean | null>(null)
  const ready = ref(false)
  const unavailable = ref(false)
  /** True se l'ultimo logout ha azzerato la sessione localmente senza
   * conferma dal server. Letto (e azzerato) dalla vista che accoglie
   * l'utente dopo il redirect, per segnalarlo senza bloccare l'uscita. */
  const logoutError = ref(false)

  let timer: ReturnType<typeof setInterval> | undefined
  let listening = false

  /** Determina lo stato dell'istanza e ripristina la sessione se presente. */
  async function bootstrap(): Promise<void> {
    unavailable.value = false
    try {
      const status = await authApi.getSetupStatus()
      initialised.value = status.initialised

      if (status.initialised) {
        try {
          const result = await authApi.me()
          user.value = result.user
          startWatchdog()
        } catch (error) {
          // 401 è normale: nessuna sessione attiva.
          if (!(error instanceof ApiProblem) || error.status !== 401) throw error
          user.value = null
          stopWatchdog()
        }
      }
    } catch (error) {
      if (error instanceof ApiProblem && error.status === 503) {
        unavailable.value = true
        ready.value = true
        return
      }
      throw error
    }
    ready.value = true
  }

  async function retryBootstrap(): Promise<void> {
    ready.value = false
    await bootstrap()
  }

  async function login(username: string, password: string): Promise<void> {
    const result = await authApi.login(username, password)
    user.value = result.user
    startWatchdog()
  }

  async function setup(payload: SetupPayload): Promise<void> {
    const result = await authApi.setupAccount(payload)
    user.value = result.user
    initialised.value = true
    startWatchdog()
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
      stopWatchdog()
    }
  }

  async function tick(): Promise<void> {
    if (document.visibilityState !== 'visible' || !user.value) return
    try {
      await authApi.refresh()
    } catch (error) {
      if (error instanceof ApiProblem && error.status === 401) {
        user.value = null
        stopWatchdog()
      }
    }
  }

  function armInterval(): void {
    if (timer !== undefined) clearInterval(timer)
    timer = setInterval(() => {
      void tick()
    }, SESSION_REFRESH_INTERVAL_MS)
  }

  function onVisibility(): void {
    if (document.visibilityState === 'visible') {
      void tick()
      armInterval()
    } else if (timer !== undefined) {
      clearInterval(timer)
      timer = undefined
    }
  }

  function startWatchdog(): void {
    stopWatchdog()
    document.addEventListener('visibilitychange', onVisibility)
    listening = true
    if (document.visibilityState === 'visible') {
      armInterval()
    }
  }

  function stopWatchdog(): void {
    if (timer !== undefined) {
      clearInterval(timer)
      timer = undefined
    }
    if (listening) {
      document.removeEventListener('visibilitychange', onVisibility)
      listening = false
    }
  }

  return {
    user,
    initialised,
    ready,
    unavailable,
    logoutError,
    bootstrap,
    retryBootstrap,
    login,
    setup,
    logout,
    stopWatchdog
  }
})

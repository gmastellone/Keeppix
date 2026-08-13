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

  async function logout(): Promise<void> {
    await authApi.logout()
    user.value = null
  }

  return { user, initialised, ready, bootstrap, login, setup, logout }
})

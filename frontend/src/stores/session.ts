import { defineStore } from 'pinia'
import { ref } from 'vue'

import * as authApi from '@/api/auth'
import type { SetupPayload, User } from '@/api/auth'
import { ApiProblem } from '@/api/client'
import { fetchLibraries } from '@/api/libraries'
import { updateUser } from '@/api/users'
import { applyProfileLocale, setLocale, type Locale } from '@/i18n'

export type { User }

/** Cookie TTL is 30 days: a ping every 12 hours, only while the tab is
 * visible, keeps the session alive without spinning all night on a Pi. */
export const SESSION_REFRESH_INTERVAL_MS = 12 * 60 * 60 * 1000

export const useSessionStore = defineStore('session', () => {
  const user = ref<User | null>(null)
  const initialised = ref<boolean | null>(null)
  /** `null` until known. Only meaningful once `user` is set — the router
   * uses `false` here to send an already-admin'd-but-library-less session
   * back into the setup wizard's library step instead of stranding it: the
   * wizard's own step state lives only in that component and doesn't
   * survive a reload. */
  const hasLibrary = ref<boolean | null>(null)
  const ready = ref(false)
  const unavailable = ref(false)
  /** True if the last logout cleared the session locally without
   * confirmation from the server. Read (and cleared) by the view that
   * greets the user after the redirect, to surface it without blocking
   * the sign-out. */
  const logoutError = ref(false)

  let timer: ReturnType<typeof setInterval> | undefined
  let listening = false

  /** A failure here (network hiccup, whatever) shouldn't block login or
   * bootstrap — worst case the router asks again on the next navigation
   * since `hasLibrary` just stays at its previous value. */
  async function refreshHasLibrary(): Promise<void> {
    try {
      hasLibrary.value = (await fetchLibraries()).length > 0
    } catch {
      // leave hasLibrary as-is
    }
  }

  /** Determines the instance's state and restores the session if present. */
  async function bootstrap(): Promise<void> {
    unavailable.value = false
    try {
      const status = await authApi.getSetupStatus()
      initialised.value = status.initialised

      if (status.initialised) {
        try {
          const result = await authApi.me()
          user.value = result.user
          applyProfileLocale(result.user.locale)
          startWatchdog()
          await refreshHasLibrary()
        } catch (error) {
          // 401 is normal: no active session.
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

  async function login(username: string, password: string, totpCode?: string): Promise<void> {
    const result = await authApi.login(username, password, totpCode)
    user.value = result.user
    applyProfileLocale(result.user.locale)
    startWatchdog()
    await refreshHasLibrary()
  }

  /** Called once `LibraryStep` creates the first library, so the router
   * stops sending this session back into the setup wizard. */
  function markLibraryCreated(): void {
    hasLibrary.value = true
  }

  async function setup(payload: SetupPayload): Promise<void> {
    const result = await authApi.setupAccount(payload)
    user.value = result.user
    applyProfileLocale(result.user.locale)
    initialised.value = true
    startWatchdog()
  }

  /**
   * Persist the UI language on the profile and keep the localStorage
   * cache in sync for the next first paint.
   */
  async function changeLocale(locale: Locale): Promise<void> {
    setLocale(locale)
    const current = user.value
    if (!current) return
    const updated = await updateUser(current.id, { locale })
    user.value = {
      ...current,
      locale: updated.locale
    }
  }

  /**
   * Account data "Save changes" button — unlike the mockup, which wasn't
   * wired to anything, this actually writes via the same
   * `PATCH /users/{id}` already used by `changeLocale`. `display_name`
   * only: email has no write path on the backend (`UserView` exposes it
   * read-only).
   */
  async function updateDisplayName(displayName: string): Promise<void> {
    const current = user.value
    if (!current) return
    const updated = await updateUser(current.id, { display_name: displayName })
    user.value = {
      ...current,
      display_name: updated.display_name
    }
  }

  /**
   * Logout is a security action: if the server-side revocation fails
   * (almost certainly a network error — the backend still responds `204`
   * for the failures it can handle), the user must not stay stuck in a
   * UI that still shows them as authenticated. Local state is cleared
   * either way, and the failure is surfaced through `logoutError` instead
   * of propagating the exception to the caller.
   */
  async function logout(): Promise<void> {
    try {
      await authApi.logout()
      logoutError.value = false
    } catch {
      logoutError.value = true
    } finally {
      user.value = null
      hasLibrary.value = null
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
    hasLibrary,
    ready,
    unavailable,
    logoutError,
    bootstrap,
    retryBootstrap,
    login,
    setup,
    markLibraryCreated,
    changeLocale,
    updateDisplayName,
    logout,
    stopWatchdog
  }
})

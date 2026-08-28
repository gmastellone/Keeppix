// A toast has three natures: success stays neutral and disappears on its
// own — it's the normal case, it doesn't deserve color or attention;
// error and partial success carry a colored accent, stay on screen
// longer (reading "what went wrong" takes more time than reading "done")
// and can carry an action, almost always "Retry" — a toast with an
// action doesn't disappear while the user is still considering it:
// hovering the mouse over it pauses the timer.
import { defineStore } from 'pinia'
import { ref } from 'vue'

import {
  TOAST_LIFE_ERROR_MS,
  TOAST_LIFE_SUCCESS_MS,
  TOAST_LIFE_WITH_ACTION_MS,
  TOAST_REMOVE_AFTER_MS,
  TOAST_SHOW_DELAY_MS
} from '@/design/tokens'
// Labels depend on the current language, but this store isn't a
// component: no `useI18n()`. `i18n.global.t` is the same instance that
// components read, imported directly (same pattern already used
// elsewhere by non-component code that needs to translate, e.g.
// `api/client.ts` for network errors).
import { i18n } from '@/i18n'

export type ToastKind = 'ok' | 'error' | 'partial'

export interface ToastAction {
  label: string
  run: () => void
}

export interface Toast {
  id: number
  message: string
  kind: ToastKind
  action?: ToastAction
  /** `false` until `TOAST_SHOW_DELAY_MS` has elapsed: drives the entry
   * transition, which would otherwise start already visible. */
  visible: boolean
}

let nextId = 0

function lifeFor(kind: ToastKind, hasAction: boolean): number {
  if (hasAction) return TOAST_LIFE_WITH_ACTION_MS
  return kind === 'ok' ? TOAST_LIFE_SUCCESS_MS : TOAST_LIFE_ERROR_MS
}

export const useToastStore = defineStore('toast', () => {
  const toasts = ref<Toast[]>([])
  const timers = new Map<number, ReturnType<typeof setTimeout>>()

  function arm(id: number, kind: ToastKind, hasAction: boolean) {
    timers.set(
      id,
      setTimeout(() => close(id), lifeFor(kind, hasAction))
    )
  }

  /** Only toasts with an action use this, on mouse hover. */
  function pause(id: number) {
    const timer = timers.get(id)
    if (timer) clearTimeout(timer)
  }

  function resume(id: number) {
    const toast = toasts.value.find((t) => t.id === id)
    if (toast) arm(id, toast.kind, Boolean(toast.action))
  }

  /** Returns a function that closes the toast early. */
  function show(message: string, opts: { kind?: ToastKind; action?: ToastAction } = {}): () => void {
    const id = nextId++
    const kind = opts.kind ?? 'ok'
    toasts.value.push({ id, message, kind, action: opts.action, visible: false })
    setTimeout(() => {
      const toast = toasts.value.find((t) => t.id === id)
      if (toast) toast.visible = true
    }, TOAST_SHOW_DELAY_MS)
    arm(id, kind, Boolean(opts.action))
    return () => close(id)
  }

  function showError(message: string, retry?: () => void): () => void {
    return show(message, { kind: 'error', action: retry ? { label: retryLabel(), run: retry } : undefined })
  }

  /** `failedCount` is always ≥ 1: a full success never produces a
   * partial-success toast. */
  function showPartial(okCount: number, failedCount: number, retry?: () => void): () => void {
    return show(partialMessage(okCount, failedCount), {
      kind: 'partial',
      action: retry ? { label: retryRemainingLabel(failedCount), run: retry } : undefined
    })
  }

  function close(id: number) {
    const timer = timers.get(id)
    if (timer) clearTimeout(timer)
    timers.delete(id)
    const toast = toasts.value.find((t) => t.id === id)
    if (toast) toast.visible = false
    setTimeout(() => {
      toasts.value = toasts.value.filter((t) => t.id !== id)
    }, TOAST_REMOVE_AFTER_MS)
  }

  function runAction(id: number) {
    const toast = toasts.value.find((t) => t.id === id)
    close(id)
    toast?.action?.run()
  }

  return { toasts, show, showError, showPartial, pause, resume, runAction, close }
})

function retryLabel(): string {
  return i18n.global.t('ui.toast.retry')
}

function retryRemainingLabel(n: number): string {
  return i18n.global.t('ui.toast.retryRemaining', { n })
}

function partialMessage(ok: number, n: number): string {
  return i18n.global.t('ui.toast.partial', { ok, total: ok + n, n }, { plural: n })
}

// SP-6/SP-28/SP-29 (Fase 11 Task 2): il toast ha tre nature (documento
// funzionale, commento del prototipo su `showToast`): il successo resta
// neutro e sparisce da solo — è il caso normale, non merita colore né
// attenzione; l'errore e la riuscita parziale portano un filetto colorato,
// restano a schermo più a lungo (leggere "cosa non ha funzionato" richiede
// più tempo che leggere "fatto") e possono avere un'azione, quasi sempre
// "Riprova" — un toast con azione non sparisce mentre ci si sta
// ragionando sopra: passandoci il mouse il timer si ferma. Ogni numero qui
// è letto da `showToast`/`showErrorToast`/`showPartialToast` in
// docs/ui/keeppix-mockup.html, non stimato.
import { defineStore } from 'pinia'
import { ref } from 'vue'

import {
  TOAST_LIFE_ERROR_MS,
  TOAST_LIFE_SUCCESS_MS,
  TOAST_LIFE_WITH_ACTION_MS,
  TOAST_REMOVE_AFTER_MS,
  TOAST_SHOW_DELAY_MS
} from '@/design/tokens'
// Le etichette dipendono dalla lingua corrente ma questo store non è un
// componente: niente `useI18n()`. `i18n.global.t` è la stessa istanza che
// i componenti leggono, importata direttamente (stesso pattern già in uso
// altrove per codice non-componente che deve tradurre, es. `api/client.ts`
// negli errori di rete).
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
  /** `false` finché non è passato `TOAST_SHOW_DELAY_MS`: pilota la
   * transizione d'ingresso, che altrimenti partirebbe già visibile. */
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

  /** Solo i toast con azione lo usano (stesso comportamento del
   * prototipo): al passaggio del mouse. */
  function pause(id: number) {
    const timer = timers.get(id)
    if (timer) clearTimeout(timer)
  }

  function resume(id: number) {
    const toast = toasts.value.find((t) => t.id === id)
    if (toast) arm(id, toast.kind, Boolean(toast.action))
  }

  /** Restituisce una funzione per chiudere il toast prima del tempo —
   * stessa forma di `showToast` nel prototipo. */
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

  /** `failedCount` sempre ≥ 1: un successo pieno non produce un toast di
   * riuscita parziale. */
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

import { mount, type VueWrapper } from '@vue/test-utils'
import { defineComponent, ref } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import ConfirmDialog from './ConfirmDialog.vue'

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

// `open` è una prop v-model obbligatoria: `defineModel` la sincronizza solo
// se un genitore reattivo la riscrive davvero dall'esterno in risposta
// all'evento emesso, non da un ascoltatore vuoto — un componente ospite con
// stato proprio, non `ConfirmDialog` montato da solo con una prop statica.
function mountHost() {
  const Host = defineComponent({
    components: { TheDialog: ConfirmDialog },
    emits: ['confirm'],
    setup(_, { emit }) {
      const open = ref(true)
      return { open, onConfirm: () => emit('confirm') }
    },
    template: `<TheDialog
      v-model:open="open"
      title="Eliminare il gruppo?"
      description="Le persone al suo interno restano."
      confirm-label="Elimina gruppo"
      @confirm="onConfirm"
    />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

describe('ConfirmDialog', () => {
  // Il `DialogPortal` di reka-ui teletrasporta sempre nel vero
  // `document.body`, non nel contenitore isolato del wrapper — senza smontare
  // esplicitamente, il markup di un test resta lì per il successivo, che
  // rischia di trovare (e cliccare) il bottone del test sbagliato.
  afterEach(() => {
    wrapper?.unmount()
    wrapper = undefined
  })

  it('focuses "Annulla", not the destructive confirm button, on open', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    mountHost()
    await tick()

    expect(document.activeElement?.textContent).toBe('Annulla')
    i18n.global.locale.value = previousLocale
  })

  it('emits confirm and closes when the destructive button is clicked', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const w = mountHost()
    await tick()

    const buttons = Array.from(document.body.querySelectorAll('button'))
    const confirmBtn = buttons.find((b) => b.textContent === 'Elimina gruppo')
    confirmBtn?.click()
    await tick()

    expect(w.emitted('confirm')).toHaveLength(1)
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    i18n.global.locale.value = previousLocale
  })

  it('closes without emitting confirm when "Annulla" is clicked', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const w = mountHost()
    await tick()

    const buttons = Array.from(document.body.querySelectorAll('button'))
    const cancelBtn = buttons.find((b) => b.textContent === 'Annulla')
    cancelBtn?.click()
    await tick()

    expect(w.emitted('confirm')).toBeUndefined()
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    i18n.global.locale.value = previousLocale
  })
})

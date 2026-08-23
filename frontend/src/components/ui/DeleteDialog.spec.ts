import { mount, type VueWrapper } from '@vue/test-utils'
import { defineComponent, ref } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import DeleteDialog, { type DeleteChoice } from './DeleteDialog.vue'

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

// Stesso motivo di ConfirmDialog.spec.ts: `open` è una prop v-model
// obbligatoria, serve un genitore reattivo vero che la riscriva in risposta
// all'evento emesso, non solo una prop statica passata al montaggio.
function mountHost() {
  const Host = defineComponent({
    components: { TheDeleteDialog: DeleteDialog },
    emits: ['choose'],
    setup(_, { emit }) {
      const open = ref(true)
      return { open, onChoose: (choice: DeleteChoice) => emit('choose', choice) }
    },
    template: `<TheDeleteDialog v-model:open="open" title='Eliminare "foto.jpg"?' @choose="onChoose" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

describe('DeleteDialog', () => {
  // Il `DialogPortal` di reka-ui teletrasporta sempre nel vero
  // `document.body` — senza smontare esplicitamente, il markup di un test
  // resta lì per il successivo, che rischia di trovare (e cliccare) il
  // bottone del test sbagliato.
  afterEach(() => {
    wrapper?.unmount()
    wrapper = undefined
  })

  it('focuses the least destructive option ("rimuovi dall\'indice"), not disk deletion', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    mountHost()
    await tick()

    expect(document.activeElement?.textContent).toContain("Rimuovi solo dall'indice")
    i18n.global.locale.value = previousLocale
  })

  it('emits the chosen option and closes', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const w = mountHost()
    await tick()

    const buttons = Array.from(document.body.querySelectorAll('button'))
    const diskBtn = buttons.find((b) => b.textContent?.includes('Elimina dal disco adesso'))
    diskBtn?.click()
    await tick()

    expect(w.emitted('choose')).toEqual([['disk']])
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    i18n.global.locale.value = previousLocale
  })

  it('closes without emitting a choice when "Annulla" is clicked', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const w = mountHost()
    await tick()

    const buttons = Array.from(document.body.querySelectorAll('button'))
    const cancelBtn = buttons.find((b) => b.textContent === 'Annulla')
    cancelBtn?.click()
    await tick()

    expect(w.emitted('choose')).toBeUndefined()
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
    i18n.global.locale.value = previousLocale
  })
})

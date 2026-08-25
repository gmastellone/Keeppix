import { mount } from '@vue/test-utils'
import { defineComponent, ref } from 'vue'
import { describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import Dialog from './Dialog.vue'

describe('Dialog', () => {
  it('renders nothing in the document when closed', () => {
    mount(Dialog, {
      props: { title: 'Titolo', open: false, 'onUpdate:open': () => {} },
      global: { plugins: [i18n] }
    })
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('renders title, description and slot content when open', async () => {
    mount(Dialog, {
      props: { title: 'Elimina foto', description: 'Non si può annullare.', open: true, 'onUpdate:open': () => {} },
      slots: { default: '<p>Corpo del dialog</p>' },
      global: { plugins: [i18n] }
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    const dialog = document.body.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.textContent).toContain('Elimina foto')
    expect(dialog?.textContent).toContain('Non si può annullare.')
    expect(dialog?.textContent).toContain('Corpo del dialog')
  })

  it('the close button is reachable and labelled', async () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    mount(Dialog, {
      props: { title: 'Titolo', open: true, 'onUpdate:open': () => {} },
      global: { plugins: [i18n] }
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    const close = document.body.querySelector('button[aria-label="Chiudi"]')
    expect(close).not.toBeNull()
    i18n.global.locale.value = previousLocale
  })

  it('Escape asks the caller to close the dialog', async () => {
    let open = true
    mount(Dialog, {
      props: {
        title: 'Titolo',
        open: true,
        'onUpdate:open': (value: boolean) => {
          open = value
        }
      },
      global: { plugins: [i18n] }
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    const dialog = document.body.querySelector('[role="dialog"]') as HTMLElement
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(open).toBe(false)
  })

  it('sends initial focus to the given element instead of the default first-tabbable one', async () => {
    const Host = defineComponent({
      components: { TheDialog: Dialog },
      setup() {
        const cancelBtn = ref<HTMLElement>()
        return { cancelBtn }
      },
      template: `
        <div>
          <button ref="cancelBtn" type="button">Annulla</button>
          <button type="button">Conferma</button>
          <TheDialog title="Conferma" :open="true" :initial-focus="cancelBtn" @update:open="() => {}" />
        </div>
      `
    })
    const wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
    await wrapper.vm.$nextTick()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(document.activeElement?.textContent).toBe('Annulla')
  })
})

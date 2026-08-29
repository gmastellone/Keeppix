import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Popover from './Popover.vue'

describe('Popover', () => {
  it('opens on trigger click and shows the content', async () => {
    const wrapper = mount(Popover, {
      slots: {
        trigger: '<button type="button">Apri</button>',
        default: '<p>Contenuto</p>'
      },
      attachTo: document.body
    })
    expect(document.body.querySelector('[role="dialog"], [data-reka-popper-content-wrapper]')).toBeNull()

    await wrapper.get('button').trigger('click')
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(document.body.textContent).toContain('Contenuto')
    wrapper.unmount()
  })

  it('closes on Escape', async () => {
    const wrapper = mount(Popover, {
      slots: {
        trigger: '<button type="button">Apri</button>',
        default: '<p>Contenuto</p>'
      },
      attachTo: document.body
    })
    await wrapper.get('button').trigger('click')
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(document.body.textContent).toContain('Contenuto')

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(document.body.textContent).not.toContain('Contenuto')
    wrapper.unmount()
  })

  it('stays open on Escape when escDismisses is false', async () => {
    const wrapper = mount(Popover, {
      props: { escDismisses: false },
      slots: {
        trigger: '<button type="button">Apri</button>',
        default: '<p>Contenuto</p>'
      },
      attachTo: document.body
    })
    await wrapper.get('button').trigger('click')
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(document.body.textContent).toContain('Contenuto')

    // `cancelable: true`: without it, `preventDefault()` inside the
    // handler is a no-op per the DOM spec — `defaultPrevented` would
    // always stay `false` and the test could never tell the two
    // conditions apart.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(document.body.textContent).toContain('Contenuto')
    wrapper.unmount()
  })

  it('respects v-model:open for programmatic control', async () => {
    const wrapper = mount(
      { components: { ThePopover: Popover }, props: ['open'], template: '<ThePopover :open="open"><template #trigger><button type="button">Apri</button></template><p>Contenuto</p></ThePopover>' },
      { props: { open: true }, attachTo: document.body }
    )
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(document.body.textContent).toContain('Contenuto')

    await wrapper.setProps({ open: false })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(document.body.textContent).not.toContain('Contenuto')
    wrapper.unmount()
  })
})

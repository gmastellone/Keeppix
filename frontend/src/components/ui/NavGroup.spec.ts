import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import NavGroup from './NavGroup.vue'

describe('NavGroup', () => {
  it('starts closed when the current view is not one of its sub-items', () => {
    const wrapper = mount(NavGroup, {
      props: { label: 'Manutenzione', active: false },
      slots: { default: '<div>Cestino</div>' }
    })

    expect(wrapper.find('button').attributes('aria-expanded')).toBe('false')
    expect(wrapper.text()).not.toContain('Cestino')
  })

  it('opens on click and rotates the arrow', async () => {
    const wrapper = mount(NavGroup, {
      props: { label: 'Manutenzione', active: false },
      slots: { default: '<div>Cestino</div>' }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.find('button').attributes('aria-expanded')).toBe('true')
    expect(wrapper.text()).toContain('Cestino')
    expect(wrapper.find('svg').classes()).toContain('rotate-180')
  })

  it('opens by itself when the current view is inside it, without ever being clicked', () => {
    const wrapper = mount(NavGroup, {
      props: { label: 'Manutenzione', active: true },
      slots: { default: '<div>Cestino</div>' }
    })

    expect(wrapper.find('button').attributes('aria-expanded')).toBe('true')
    expect(wrapper.text()).toContain('Cestino')
  })

  it('does not close from a click while the current view is inside it', async () => {
    const wrapper = mount(NavGroup, {
      props: { label: 'Manutenzione', active: true },
      slots: { default: '<div>Cestino</div>' }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.find('button').attributes('aria-expanded')).toBe('true')
    expect(wrapper.text()).toContain('Cestino')
  })
})

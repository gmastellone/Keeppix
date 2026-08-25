import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import BusyButton from './BusyButton.vue'

describe('BusyButton', () => {
  it('renders its label and is enabled when not busy', () => {
    const wrapper = mount(BusyButton, { slots: { default: 'Elimina' } })

    expect(wrapper.text()).toContain('Elimina')
    expect(wrapper.find('button').attributes('disabled')).toBeUndefined()
    expect(wrapper.find('.spinner').exists()).toBe(false)
  })

  it('blocks double-submit and marks aria-busy while busy', () => {
    const wrapper = mount(BusyButton, { props: { busy: true }, slots: { default: 'Elimina' } })

    expect(wrapper.find('button').attributes('disabled')).toBeDefined()
    expect(wrapper.find('button').attributes('aria-busy')).toBe('true')
  })

  it('keeps the label alongside the spinner when busy but not icon-only', () => {
    const wrapper = mount(BusyButton, { props: { busy: true }, slots: { default: 'Elimina' } })

    expect(wrapper.text()).toContain('Elimina')
    expect(wrapper.find('.spinner').exists()).toBe(true)
  })

  it('replaces the icon with the spinner when busy and icon-only', () => {
    const wrapper = mount(BusyButton, {
      props: { busy: true, iconOnly: true },
      slots: { default: '<svg data-testid="icon" />' }
    })

    expect(wrapper.find('[data-testid="icon"]').exists()).toBe(false)
    expect(wrapper.find('.spinner').exists()).toBe(true)
  })

  it('shows the icon normally when icon-only but not busy', () => {
    const wrapper = mount(BusyButton, {
      props: { iconOnly: true },
      slots: { default: '<svg data-testid="icon" />' }
    })

    expect(wrapper.find('[data-testid="icon"]').exists()).toBe(true)
    expect(wrapper.find('.spinner').exists()).toBe(false)
  })
})

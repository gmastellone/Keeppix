import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import SelectAllVisible from './SelectAllVisible.vue'

describe('SelectAllVisible', () => {
  it('disappears entirely when nothing is visible — no disabled variant', () => {
    const wrapper = mount(SelectAllVisible, {
      props: { visibleCount: 0 },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('renders when at least one item is visible', () => {
    const wrapper = mount(SelectAllVisible, {
      props: { visibleCount: 1 },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('button').exists()).toBe(true)
  })

  it('has the tooltip label and the more explicit aria-label the document distinguishes', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectAllVisible, {
      props: { visibleCount: 4 },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('button').attributes('aria-label')).toBe('Seleziona tutto quello che vedi')
    expect(wrapper.text()).toContain('Seleziona tutto')
    i18n.global.locale.value = previousLocale
  })

  it('emits select-all when clicked', async () => {
    const wrapper = mount(SelectAllVisible, {
      props: { visibleCount: 4 },
      global: { plugins: [i18n] }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.emitted('select-all')).toHaveLength(1)
  })
})

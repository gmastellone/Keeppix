import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import SelectionBar from './SelectionBar.vue'

describe('SelectionBar', () => {
  it('hides the toolbar at zero selected — the mode switches off by itself', () => {
    const wrapper = mount(SelectionBar, {
      props: { count: 0, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false)
  })

  it('keeps the live-region node mounted even when the toolbar is hidden — otherwise a clear announcement could never fire', () => {
    const wrapper = mount(SelectionBar, {
      props: { count: 0, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('[aria-live="polite"]').exists()).toBe(true)
  })

  it('shows the singular count for exactly one selected item', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectionBar, {
      props: { count: 1, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('b').text()).toBe('1 selezionata')
    i18n.global.locale.value = previousLocale
  })

  it('shows the plural count for more than one selected item', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectionBar, {
      props: { count: 3, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('b').text()).toBe('3 selezionate')
    i18n.global.locale.value = previousLocale
  })

  it('the "select all" label never changes, even though it can deselect', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectionBar, {
      props: { count: 5, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    const link = wrapper.findAll('button')[1]
    expect(link?.text()).toBe('Seleziona tutte')
    i18n.global.locale.value = previousLocale
  })

  it('emits clear when the cancel button is clicked', async () => {
    const wrapper = mount(SelectionBar, {
      props: { count: 2, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    await wrapper.findAll('button')[0]?.trigger('click')

    expect(wrapper.emitted('clear')).toHaveLength(1)
  })

  it('emits select-all when the link is clicked', async () => {
    const wrapper = mount(SelectionBar, {
      props: { count: 2, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    await wrapper.findAll('button')[1]?.trigger('click')

    expect(wrapper.emitted('select-all')).toHaveLength(1)
  })

  it('renders the caller-composed action buttons in the default slot', () => {
    const wrapper = mount(SelectionBar, {
      props: { count: 2, ariaLabel: 'Azioni sulla selezione' },
      slots: { default: '<button type="button">Elimina</button>' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.text()).toContain('Elimina')
  })

  it('announces the italian selected-count text when selecting', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectionBar, {
      props: { count: 2, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('[aria-live="polite"]').text()).toBe('2 foto selezionate')
    i18n.global.locale.value = previousLocale
  })

  it('announces "Selezione annullata" when the count reaches zero', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(SelectionBar, {
      props: { count: 0, ariaLabel: 'Azioni sulla selezione' },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('[aria-live="polite"]').text()).toBe('Selezione annullata')
    i18n.global.locale.value = previousLocale
  })
})

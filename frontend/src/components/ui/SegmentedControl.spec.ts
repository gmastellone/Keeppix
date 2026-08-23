import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import SegmentedControl from './SegmentedControl.vue'

const OPTIONS = [
  { value: '', label: 'Non modificare' },
  { value: 'pick', label: 'Pick' },
  { value: 'reject', label: 'Scarta' },
  { value: 'none', label: 'Nessuno' }
]

describe('SegmentedControl', () => {
  it('only the checked option is tabbable — the rest are roving-tabindex -1', () => {
    const wrapper = mount(SegmentedControl, {
      props: { options: OPTIONS, modelValue: 'pick', 'onUpdate:modelValue': () => {} }
    })

    const radios = wrapper.findAll('[role="radio"]')
    expect(radios.map((r) => r.attributes('tabindex'))).toEqual(['-1', '0', '-1', '-1'])
    expect(radios.map((r) => r.attributes('aria-checked'))).toEqual(['false', 'true', 'false', 'false'])
  })

  it('clicking an option selects it', async () => {
    const wrapper = mount(SegmentedControl, {
      props: { options: OPTIONS, modelValue: '', 'onUpdate:modelValue': () => {} }
    })

    await wrapper.findAll('[role="radio"]')[2]?.trigger('click')

    expect(wrapper.emitted('update:modelValue')).toEqual([['reject']])
  })

  it('ArrowRight moves selection and focus to the next option, wrapping at the end', async () => {
    const wrapper = mount(SegmentedControl, {
      props: { options: OPTIONS, modelValue: 'none', 'onUpdate:modelValue': () => {} },
      attachTo: document.body
    })

    await wrapper.findAll('[role="radio"]')[3]?.trigger('keydown', { key: 'ArrowRight' })

    expect(wrapper.emitted('update:modelValue')).toEqual([['']])
    expect(document.activeElement?.textContent).toBe('Non modificare')
    wrapper.unmount()
  })

  it('ArrowLeft moves selection and focus to the previous option, wrapping at the start', async () => {
    const wrapper = mount(SegmentedControl, {
      props: { options: OPTIONS, modelValue: '', 'onUpdate:modelValue': () => {} },
      attachTo: document.body
    })

    await wrapper.findAll('[role="radio"]')[0]?.trigger('keydown', { key: 'ArrowLeft' })

    expect(wrapper.emitted('update:modelValue')).toEqual([['none']])
    expect(document.activeElement?.textContent).toBe('Nessuno')
    wrapper.unmount()
  })
})

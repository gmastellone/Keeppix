import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import ProvenanceBadge from './ProvenanceBadge.vue'

describe('ProvenanceBadge', () => {
  it('renders nothing for a human-confirmed assignment — no marker means no ambiguity', () => {
    const wrapper = mount(ProvenanceBadge, { props: { origin: 'human' }, global: { plugins: [i18n] } })

    expect(wrapper.html()).toBe('<!--v-if-->')
  })

  it('shows the "IA" marker with an explanatory label for an AI-origin assignment', () => {
    const previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
    const wrapper = mount(ProvenanceBadge, { props: { origin: 'ai' }, global: { plugins: [i18n] } })

    expect(wrapper.text()).toBe('IA')
    expect(wrapper.attributes('aria-label')).toContain('intelligenza artificiale')
    i18n.global.locale.value = previousLocale
  })
})

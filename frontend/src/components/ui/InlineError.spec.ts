import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'
import type { ErrorNature } from '@/errors/classify'

import InlineError from './InlineError.vue'

const ALL_NATURES: ErrorNature[] = ['unreachable', 'permission-denied', 'file-missing', 'timeout', 'unknown']

describe('InlineError', () => {
  let previousLocale: typeof i18n.global.locale.value

  beforeEach(() => {
    previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
  })

  afterEach(() => {
    i18n.global.locale.value = previousLocale
  })

  it.each(['unreachable', 'permission-denied'] as const)(
    'shows "Riprova" for the retryable nature %s',
    (nature) => {
      const wrapper = mount(InlineError, { props: { nature }, global: { plugins: [i18n] } })
      expect(wrapper.find('button').exists()).toBe(true)
    }
  )

  it.each(['file-missing', 'timeout', 'unknown'] as const)(
    'has no "Riprova" for the non-retryable nature %s',
    (nature) => {
      const wrapper = mount(InlineError, { props: { nature }, global: { plugins: [i18n] } })
      expect(wrapper.find('button').exists()).toBe(false)
    }
  )

  it('clicking Riprova emits retry', async () => {
    const wrapper = mount(InlineError, { props: { nature: 'permission-denied' }, global: { plugins: [i18n] } })
    await wrapper.find('button').trigger('click')
    expect(wrapper.emitted('retry')).toHaveLength(1)
  })

  it('is announced as an alert and shows the nature title', () => {
    const wrapper = mount(InlineError, { props: { nature: 'file-missing' }, global: { plugins: [i18n] } })
    expect(wrapper.attributes('role')).toBe('alert')
    expect(wrapper.text()).toContain('File non trovato')
  })

  it('every nature renders non-empty text in both languages', () => {
    for (const locale of ['it', 'en'] as const) {
      i18n.global.locale.value = locale
      for (const nature of ALL_NATURES) {
        const wrapper = mount(InlineError, { props: { nature }, global: { plugins: [i18n] } })
        expect(wrapper.text().length).toBeGreaterThan(0)
        expect(wrapper.text()).not.toContain('errors.')
      }
    }
  })
})

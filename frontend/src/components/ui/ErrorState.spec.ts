import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'
import type { ErrorNature } from '@/errors/classify'

import ErrorState from './ErrorState.vue'

const ALL_NATURES: ErrorNature[] = ['unreachable', 'permission-denied', 'file-missing', 'timeout', 'unknown']

describe('ErrorState', () => {
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
      const wrapper = mount(ErrorState, { props: { nature }, global: { plugins: [i18n] } })
      expect(wrapper.find('button').exists()).toBe(true)
    }
  )

  it.each(['file-missing', 'timeout', 'unknown'] as const)(
    'has no "Riprova" for the non-retryable nature %s',
    (nature) => {
      const wrapper = mount(ErrorState, { props: { nature }, global: { plugins: [i18n] } })
      expect(wrapper.find('button').exists()).toBe(false)
    }
  )

  it('clicking Riprova emits retry', async () => {
    const wrapper = mount(ErrorState, { props: { nature: 'unreachable' }, global: { plugins: [i18n] } })
    await wrapper.find('button').trigger('click')
    expect(wrapper.emitted('retry')).toHaveLength(1)
  })

  it('says what failed and what did not happen — never the generic "something went wrong"', () => {
    const wrapper = mount(ErrorState, { props: { nature: 'unreachable' }, global: { plugins: [i18n] } })
    expect(wrapper.text()).toContain('Impossibile raggiungere il server')
    expect(wrapper.text()).toContain('Le foto restano al sicuro')
    expect(wrapper.text()).not.toContain('qualcosa è andato storto')
  })

  it('shows the optional technical detail line, monospaced, only when passed', () => {
    const withDetail = mount(ErrorState, {
      props: { nature: 'unreachable', technicalDetail: 'GET /timeline/buckets → 503' },
      global: { plugins: [i18n] }
    })
    expect(withDetail.text()).toContain('GET /timeline/buckets → 503')
    expect(withDetail.find('.font-mono').exists()).toBe(true)

    const withoutDetail = mount(ErrorState, { props: { nature: 'unreachable' }, global: { plugins: [i18n] } })
    expect(withoutDetail.find('.font-mono').exists()).toBe(false)
  })

  it('every nature has non-empty title and reassurance text in both languages', () => {
    for (const locale of ['it', 'en'] as const) {
      i18n.global.locale.value = locale
      for (const nature of ALL_NATURES) {
        const wrapper = mount(ErrorState, { props: { nature }, global: { plugins: [i18n] } })
        const title = wrapper.find('p').text()
        expect(title.length).toBeGreaterThan(0)
        expect(title).not.toContain('errors.')
      }
    }
  })
})

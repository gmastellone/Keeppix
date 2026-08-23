import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import QuickFilter, { type QuickFilterDimension } from './QuickFilter.vue'

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

const SMALL_DIMENSIONS: QuickFilterDimension[] = [
  { id: 'type', label: 'Tipo', options: [{ value: 'raw', label: 'RAW' }, { value: 'jpeg', label: 'JPEG' }] }
]

function manyOptions(count: number) {
  return Array.from({ length: count }, (_, i) => ({ value: `tag-${i}`, label: `Tag ${i}` }))
}

async function mountFilter(props: {
  dimensions: QuickFilterDimension[]
  selection?: Record<string, Set<string>>
  resultCount?: number
}) {
  wrapper = mount(QuickFilter, {
    props: {
      dimensions: props.dimensions,
      selection: props.selection ?? {},
      resultCount: props.resultCount ?? 42,
      'onUpdate:selection': () => {}
    },
    global: { plugins: [i18n] },
    attachTo: document.body
  })
  await wrapper.find('button').trigger('click')
  await tick()
  return wrapper
}

describe('QuickFilter', () => {
  let previousLocale: typeof i18n.global.locale.value

  beforeEach(() => {
    previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = undefined
    i18n.global.locale.value = previousLocale
  })

  it('shows no badge when nothing is selected', () => {
    wrapper = mount(QuickFilter, {
      props: { dimensions: SMALL_DIMENSIONS, selection: {}, resultCount: 10, 'onUpdate:selection': () => {} },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('button span').exists()).toBe(false)
  })

  it('the badge sums values across all dimensions, not the number of active dimensions', () => {
    wrapper = mount(QuickFilter, {
      props: {
        dimensions: SMALL_DIMENSIONS,
        selection: { type: new Set(['raw', 'jpeg']) },
        resultCount: 10,
        'onUpdate:selection': () => {}
      },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('button span').text()).toBe('2')
  })

  it('opens the panel on trigger click and lists the dimension options as chips', async () => {
    await mountFilter({ dimensions: SMALL_DIMENSIONS })

    expect(document.body.textContent).toContain('RAW')
    expect(document.body.textContent).toContain('JPEG')
  })

  it('clicking a chip emits the toggled selection', async () => {
    await mountFilter({ dimensions: SMALL_DIMENSIONS })

    const rawChip = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'RAW')
    rawChip?.click()
    await tick()

    const emitted = wrapper?.emitted('update:selection')
    expect(emitted).toHaveLength(1)
    expect((emitted![0][0] as Record<string, Set<string>>).type).toEqual(new Set(['raw']))
  })

  it('"Cancella tutto" only appears when something is selected', async () => {
    await mountFilter({ dimensions: SMALL_DIMENSIONS, selection: {} })
    expect(document.body.textContent).not.toContain('Cancella tutto')
    wrapper?.unmount()

    await mountFilter({ dimensions: SMALL_DIMENSIONS, selection: { type: new Set(['raw']) } })
    expect(document.body.textContent).toContain('Cancella tutto')
  })

  it('the search field only appears for a dimension with more than 8 options', async () => {
    const dimensions: QuickFilterDimension[] = [
      { id: 'small', label: 'Piccola', options: [{ value: 'a', label: 'A' }] },
      { id: 'big', label: 'Grande', options: manyOptions(9) }
    ]
    await mountFilter({ dimensions })

    const inputs = document.body.querySelectorAll('input[type="text"]')
    expect(inputs).toHaveLength(1)
    expect((inputs[0] as HTMLInputElement).placeholder).toBe('Cerca in 9…')
  })

  it('selected options are never filtered out by a search term that no longer matches them', async () => {
    const dimensions: QuickFilterDimension[] = [{ id: 'big', label: 'Grande', options: manyOptions(9) }]
    await mountFilter({ dimensions, selection: { big: new Set(['tag-0']) } })

    const input = document.body.querySelector('input[type="text"]') as HTMLInputElement
    input.value = 'zzz-no-match'
    input.dispatchEvent(new Event('input'))
    await tick()

    expect(document.body.textContent).toContain('Tag 0')
  })

  it('the footer switches between "total" and "with these filters" based on whether anything is active', async () => {
    await mountFilter({ dimensions: SMALL_DIMENSIONS, selection: {}, resultCount: 7 })
    expect(document.body.textContent).toContain('7 foto in totale')
    wrapper?.unmount()

    await mountFilter({ dimensions: SMALL_DIMENSIONS, selection: { type: new Set(['raw']) }, resultCount: 3 })
    expect(document.body.textContent).toContain('3 foto con questi filtri')
  })
})

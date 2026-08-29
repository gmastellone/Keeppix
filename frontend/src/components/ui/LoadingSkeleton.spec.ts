import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import LoadingSkeleton from './LoadingSkeleton.vue'

describe('LoadingSkeleton', () => {
  it('renders a justified grid of skeleton tiles, hidden from assistive tech', () => {
    const wrapper = mount(LoadingSkeleton, { props: { count: 6 }, global: { plugins: [i18n] } })

    const tiles = wrapper.findAll('.skel')
    expect(tiles).toHaveLength(6)
    expect(wrapper.attributes('aria-hidden')).toBe('true')
  })

  it('varies tile aspect ratios instead of repeating a single shape', () => {
    const wrapper = mount(LoadingSkeleton, { props: { count: 4 }, global: { plugins: [i18n] } })

    const flexValues = wrapper.findAll('.skel').map((tile) => (tile.attributes('style') ?? '').match(/flex:\s*([\d.]+)/)?.[1])
    expect(new Set(flexValues).size).toBeGreaterThan(1)
  })

  it('renders two skeleton months as a single announced status region in stream mode', () => {
    const wrapper = mount(LoadingSkeleton, {
      props: { variant: 'stream', count: 20 },
      global: { plugins: [i18n] }
    })

    const status = wrapper.find('[role="status"]')
    expect(status.exists()).toBe(true)
    expect(status.attributes('aria-hidden')).toBeUndefined()
    // The inner grids stay hidden: the only announcement is the status
    // as a whole, not tile by tile.
    expect(wrapper.findAll('[aria-hidden="true"]')).toHaveLength(2)
  })
})

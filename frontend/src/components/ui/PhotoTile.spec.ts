import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import PhotoTile from './PhotoTile.vue'

const BASE_PROPS = {
  thumbnailUrl: '/thumb.jpg',
  filename: 'DSC08431.ARW',
  dateLabel: '12 luglio 2026',
  isFavorite: false,
  stackType: 'jpeg' as const,
  selected: false,
  selectionMode: false
}

describe('PhotoTile', () => {
  let previousLocale: typeof i18n.global.locale.value

  beforeEach(() => {
    previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
  })

  afterEach(() => {
    i18n.global.locale.value = previousLocale
  })

  it('opens on click outside selection mode', async () => {
    const wrapper = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })

    await wrapper.findAll('button')[0]?.trigger('click')

    expect(wrapper.emitted('open')).toHaveLength(1)
    expect(wrapper.emitted('toggle-select')).toBeUndefined()
  })

  it('clicking the tile body toggles selection instead of opening once selection mode is active', async () => {
    const wrapper = mount(PhotoTile, {
      props: { ...BASE_PROPS, selectionMode: true },
      global: { plugins: [i18n] }
    })

    await wrapper.findAll('button')[0]?.trigger('click')

    expect(wrapper.emitted('toggle-select')).toHaveLength(1)
    expect(wrapper.emitted('open')).toBeUndefined()
  })

  it('the accessible label includes the filename, date, and a favorited suffix only when favorited', () => {
    const wrapper = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })
    expect(wrapper.findAll('button')[0]?.attributes('aria-label')).toBe('Apri DSC08431.ARW, 12 luglio 2026')

    const favWrapper = mount(PhotoTile, {
      props: { ...BASE_PROPS, isFavorite: true },
      global: { plugins: [i18n] }
    })
    expect(favWrapper.findAll('button')[0]?.attributes('aria-label')).toBe(
      'Apri DSC08431.ARW, 12 luglio 2026, preferita'
    )
  })

  it('shows the RAW+JPEG badge for a raw_jpeg stack and RAW for raw_only, none for jpeg', () => {
    const rawJpeg = mount(PhotoTile, {
      props: { ...BASE_PROPS, stackType: 'raw_jpeg' as const },
      global: { plugins: [i18n] }
    })
    expect(rawJpeg.text()).toContain('RAW+JPEG')

    const rawOnly = mount(PhotoTile, {
      props: { ...BASE_PROPS, stackType: 'raw_only' as const },
      global: { plugins: [i18n] }
    })
    expect(rawOnly.text()).toContain('RAW')
    expect(rawOnly.text()).not.toContain('RAW+JPEG')

    const jpeg = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })
    expect(jpeg.text()).not.toContain('RAW')
  })

  it('the checkbox reflects selected state and emits toggle-select without opening', async () => {
    const wrapper = mount(PhotoTile, {
      props: { ...BASE_PROPS, selected: true },
      global: { plugins: [i18n] }
    })
    const checkbox = wrapper.find('[role="checkbox"]')
    expect(checkbox.attributes('aria-checked')).toBe('true')

    await checkbox.trigger('click')

    expect(wrapper.emitted('toggle-select')).toHaveLength(1)
    expect(wrapper.emitted('open')).toBeUndefined()
  })

  it('the favorite button label and icon fill reflect isFavorite, and it is absent during selection mode', () => {
    const notFav = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })
    const favBtn = notFav.findAll('button').at(-1)
    expect(favBtn?.attributes('aria-label')).toBe('Aggiungi ai preferiti')

    const fav = mount(PhotoTile, { props: { ...BASE_PROPS, isFavorite: true }, global: { plugins: [i18n] } })
    expect(fav.findAll('button').at(-1)?.attributes('aria-label')).toBe('Rimuovi dai preferiti')

    const selecting = mount(PhotoTile, {
      props: { ...BASE_PROPS, selectionMode: true },
      global: { plugins: [i18n] }
    })
    // Solo due bottoni restano (apri, cerchietto): il cuoricino non è nel DOM.
    expect(selecting.findAll('button')).toHaveLength(2)
  })

  it('clicking the favorite button emits toggle-favorite without opening the tile', async () => {
    const wrapper = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })

    await wrapper.findAll('button').at(-1)?.trigger('click')

    expect(wrapper.emitted('toggle-favorite')).toHaveLength(1)
    expect(wrapper.emitted('open')).toBeUndefined()
  })

  it('a long press of 500ms enters selection and suppresses the synthetic click that follows', async () => {
    vi.useFakeTimers()
    const wrapper = mount(PhotoTile, {
      props: { ...BASE_PROPS, enableLongPress: true },
      global: { plugins: [i18n] }
    })
    const openBtn = wrapper.findAll('button')[0]!

    await openBtn.trigger('pointerdown')
    vi.advanceTimersByTime(500)
    await openBtn.trigger('click') // il click sintetico dopo il rilascio

    expect(wrapper.emitted('toggle-select')).toHaveLength(1)
    expect(wrapper.emitted('open')).toBeUndefined()
    vi.useRealTimers()
  })

  it('releasing before 500ms cancels the long press — a normal tap still opens', async () => {
    vi.useFakeTimers()
    const wrapper = mount(PhotoTile, {
      props: { ...BASE_PROPS, enableLongPress: true },
      global: { plugins: [i18n] }
    })
    const openBtn = wrapper.findAll('button')[0]!

    await openBtn.trigger('pointerdown')
    vi.advanceTimersByTime(200)
    await openBtn.trigger('pointerup')
    vi.advanceTimersByTime(300)
    await openBtn.trigger('click')

    expect(wrapper.emitted('toggle-select')).toBeUndefined()
    expect(wrapper.emitted('open')).toHaveLength(1)
    vi.useRealTimers()
  })

  it('long press is inert unless the caller enables it', async () => {
    vi.useFakeTimers()
    const wrapper = mount(PhotoTile, { props: BASE_PROPS, global: { plugins: [i18n] } })
    const openBtn = wrapper.findAll('button')[0]!

    await openBtn.trigger('pointerdown')
    vi.advanceTimersByTime(500)

    expect(wrapper.emitted('toggle-select')).toBeUndefined()
    vi.useRealTimers()
  })
})

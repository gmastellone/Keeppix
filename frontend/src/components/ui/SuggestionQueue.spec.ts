import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { i18n } from '@/i18n'

import SuggestionQueue from './SuggestionQueue.vue'

const THUMBS = [
  { id: 'p1', thumbnailUrl: '/p1.jpg' },
  { id: 'p2', thumbnailUrl: '/p2.jpg' }
]

describe('SuggestionQueue', () => {
  let previousLocale: typeof i18n.global.locale.value

  beforeEach(() => {
    previousLocale = i18n.global.locale.value
    i18n.global.locale.value = 'it'
  })

  afterEach(() => {
    i18n.global.locale.value = previousLocale
  })

  it('shows the label in guillemets and the correct singular/plural count', () => {
    const one = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 1, thumbnails: [THUMBS[0]!] },
      global: { plugins: [i18n] }
    })
    expect(one.text()).toContain('«Paesaggi»')
    expect(one.text()).toContain('1 proposta')

    const many = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 2, thumbnails: THUMBS },
      global: { plugins: [i18n] }
    })
    expect(many.text()).toContain('2 proposte')
  })

  it('shows the color dot only for a tag group, not when color is absent (faces)', () => {
    const tagGroup = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 1, color: '#3B82C4', thumbnails: [THUMBS[0]!] },
      global: { plugins: [i18n] }
    })
    expect(tagGroup.find('span[style*="background"]').exists()).toBe(true)

    const faceGroup = mount(SuggestionQueue, {
      props: { label: 'Marta', count: 1, thumbnails: [THUMBS[0]!] },
      global: { plugins: [i18n] }
    })
    expect(faceGroup.find('span[style*="background"]').exists()).toBe(false)
  })

  it('emits confirm-all and reject-all from the group buttons', async () => {
    const wrapper = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 2, thumbnails: THUMBS },
      global: { plugins: [i18n] }
    })
    const buttons = wrapper.findAll('button')

    await buttons[0]?.trigger('click')
    await buttons[1]?.trigger('click')

    expect(wrapper.emitted('confirm-all')).toHaveLength(1)
    expect(wrapper.emitted('reject-all')).toHaveLength(1)
  })

  it('confirming or rejecting a single thumbnail emits its id', async () => {
    const wrapper = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 2, thumbnails: THUMBS },
      global: { plugins: [i18n] }
    })

    const confirmBtn = wrapper.find('[aria-label="Conferma"]')
    await confirmBtn.trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([['p1']])

    const rejectBtns = wrapper.findAll('[aria-label="Rifiuta"]')
    await rejectBtns[1]?.trigger('click')
    expect(wrapper.emitted('reject')).toEqual([['p2']])
  })

  it('renders one thumbnail per proposal with the "IA" badge', () => {
    const wrapper = mount(SuggestionQueue, {
      props: { label: 'Paesaggi', count: 2, thumbnails: THUMBS },
      global: { plugins: [i18n] }
    })

    expect(wrapper.findAll('img')).toHaveLength(2)
    expect(wrapper.text()).toContain('IA')
  })

  it('exposes the extra-actions scoped slot with each thumbnail id, for the faces-only third button', () => {
    const wrapper = mount(SuggestionQueue, {
      props: { label: 'Marta', count: 1, thumbnails: [THUMBS[0]!] },
      slots: {
        'extra-actions': `<template #extra-actions="{ id }"><button type="button" class="not-face">{{ id }}</button></template>`
      },
      global: { plugins: [i18n] }
    })

    expect(wrapper.find('.not-face').text()).toBe('p1')
  })
})

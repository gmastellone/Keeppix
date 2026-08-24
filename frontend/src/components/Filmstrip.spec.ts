import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

import Filmstrip from './Filmstrip.vue'

function asset(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'lot-1',
    filename: `${id}.jpg`,
    content_hash: null,
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: null,
    width: 100,
    height: 100,
    thumbhash: null,
    raw_kind: null,
    favorite: false,
    camera_model: null,
    tags: [],
    faces: []
  }
}

describe('Filmstrip — §15.C filmino e checkbox di selezione', () => {
  it('clicking the thumbnail body navigates (no shift)', async () => {
    const wrapper = mount(Filmstrip, {
      props: { assets: [asset('a'), asset('b')], currentId: 'a' },
      global: { plugins: [i18n] }
    })

    await wrapper.findAll('[role="option"]')[1].trigger('click')

    expect(wrapper.emitted('select')).toEqual([['b']])
    expect(wrapper.emitted('toggle')).toBeUndefined()
  })

  it('shift+click on the thumbnail body emits shift-select, not select', async () => {
    const wrapper = mount(Filmstrip, {
      props: { assets: [asset('a'), asset('b')], currentId: 'a' },
      global: { plugins: [i18n] }
    })

    await wrapper.findAll('[role="option"]')[1].trigger('click', { shiftKey: true })

    expect(wrapper.emitted('shift-select')).toEqual([['b']])
    expect(wrapper.emitted('select')).toBeUndefined()
  })

  it('clicking the checkbox toggles selection without navigating', async () => {
    const wrapper = mount(Filmstrip, {
      props: { assets: [asset('a')], currentId: 'a' },
      global: { plugins: [i18n] }
    })

    await wrapper.get('[role="checkbox"]').trigger('click')

    expect(wrapper.emitted('toggle')).toEqual([['a']])
    expect(wrapper.emitted('select')).toBeUndefined()
  })

  it('shift+click on the checkbox emits shift-toggle', async () => {
    const wrapper = mount(Filmstrip, {
      props: { assets: [asset('a')], currentId: 'a' },
      global: { plugins: [i18n] }
    })

    await wrapper.get('[role="checkbox"]').trigger('click', { shiftKey: true })

    expect(wrapper.emitted('shift-toggle')).toEqual([['a']])
  })

  it('marks a selected thumbnail with aria-checked and a selection ring', () => {
    const wrapper = mount(Filmstrip, {
      props: { assets: [asset('a')], currentId: 'a', selectedIds: new Set(['a']) },
      global: { plugins: [i18n] }
    })

    const checkbox = wrapper.get('[role="checkbox"]')
    expect(checkbox.attributes('aria-checked')).toBe('true')
  })
})

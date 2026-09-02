import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import type { Person } from '@/api/persons'
import { i18n } from '@/i18n'

import PersonCard from './PersonCard.vue'

function person(overrides: Partial<Person> = {}): Person {
  return {
    id: 'p1',
    name: 'Rosanna',
    hidden: false,
    face_count: 488,
    ...overrides
  }
}

describe('PersonCard', () => {
  it('renders the avatar as a block element — a bare <span> defaults to display:inline, on which width/height are no-ops', () => {
    const wrapper = mount(PersonCard, {
      props: { person: person(), cover: null, selected: false },
      global: { plugins: [i18n] }
    })

    const avatar = wrapper.find('span[aria-hidden="true"]')
    expect(avatar.exists()).toBe(true)
    expect(avatar.classes()).toContain('block')
    expect(avatar.classes()).toContain('h-[78px]')
    expect(avatar.classes()).toContain('w-[78px]')
  })

  it('shows the display name and photo count', () => {
    const wrapper = mount(PersonCard, {
      props: { person: person({ name: 'Rosanna', face_count: 488 }), cover: null, selected: false },
      global: { plugins: [i18n] }
    })

    expect(wrapper.text()).toContain('Rosanna')
    expect(wrapper.text()).toContain('488')
  })
})

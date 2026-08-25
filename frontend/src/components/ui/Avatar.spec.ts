import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Avatar from './Avatar.vue'

describe('Avatar', () => {
  it('shows the initials, one letter per word, uppercase, capped at two', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone' } })

    expect(wrapper.text()).toBe('GM')
  })

  it('caps at two initials for a name with more than two words', () => {
    const wrapper = mount(Avatar, { props: { name: 'Maria Grazia Rossi' } })

    expect(wrapper.text()).toBe('MG')
  })

  it('defaults to the brand accent color when none is given', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone' } })

    expect(wrapper.attributes('style')).toContain('background: var(--color-accent)')
  })

  it('uses the given color instead of the default when passed one', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone', color: '#3B82C4' } })

    expect(wrapper.attributes('style')).toContain('background: rgb(59, 130, 196)')
  })

  it('is always white text, never the theme accent-text color', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone' } })

    expect(wrapper.classes()).toContain('text-white')
  })

  it('renders the small size (28px/12px) by default, matching the sidebar footer', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone' } })

    expect(wrapper.attributes('style')).toContain('width: 28px')
    expect(wrapper.attributes('style')).toContain('font-size: 12px')
  })

  it('renders the large size (56px/20px) for the Profilo avatar', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone', size: 'lg' } })

    expect(wrapper.attributes('style')).toContain('width: 56px')
    expect(wrapper.attributes('style')).toContain('font-size: 20px')
  })

  it('exposes the full name for assistive tech via aria-label', () => {
    const wrapper = mount(Avatar, { props: { name: 'Giovanni Mastellone' } })

    expect(wrapper.attributes('aria-label')).toBe('Giovanni Mastellone')
  })
})

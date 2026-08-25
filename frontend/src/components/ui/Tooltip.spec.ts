import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Tooltip from './Tooltip.vue'

describe('Tooltip', () => {
  it('renders the slotted trigger and the tip text', () => {
    const wrapper = mount(Tooltip, {
      props: { label: 'Elimina' },
      slots: { default: '<button aria-label="Elimina">🗑</button>' }
    })

    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Elimina')
  })

  it('hides the bubble from assistive tech — the trigger carries its own aria-label', () => {
    const wrapper = mount(Tooltip, {
      props: { label: 'Elimina' },
      slots: { default: '<button aria-label="Elimina">🗑</button>' }
    })

    const bubble = wrapper.find('[aria-hidden="true"]')
    expect(bubble.exists()).toBe(true)
    expect(bubble.text()).toBe('Elimina')
  })
})

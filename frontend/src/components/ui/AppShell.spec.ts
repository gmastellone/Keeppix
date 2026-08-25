import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'

import AppShell from './AppShell.vue'

let changeHandlers: Array<(event: MediaQueryListEvent) => void> = []
let setMatches: (value: boolean) => void = () => {}

function installMatchMedia(initialMatches: boolean) {
  changeHandlers = []
  let matches = initialMatches
  setMatches = (value: boolean) => {
    matches = value
  }
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      // Un vero `MediaQueryList` legge sempre lo stato corrente: il
      // gestore di `sync()` in AppShell.vue non guarda l'evento, guarda
      // `query.matches` — questo getter deve quindi restare "vivo".
      get matches() {
        return matches
      },
      media: '(max-width: 767px)',
      onchange: null,
      addEventListener: (_event: string, handler: (event: MediaQueryListEvent) => void) => {
        changeHandlers.push(handler)
      },
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('AppShell', () => {
  it('renders the desktop slots (sidebar/topbar) above the max-width:767px threshold', () => {
    installMatchMedia(false)
    const wrapper = mount(AppShell, {
      slots: {
        sidebar: '<div>Sidebar</div>',
        topbar: '<div>Topbar</div>',
        default: '<div>Contenuto</div>',
        'mobile-header': '<div>Header mobile</div>',
        'mobile-tabbar': '<div>Tabbar mobile</div>'
      }
    })

    expect(wrapper.text()).toContain('Sidebar')
    expect(wrapper.text()).toContain('Topbar')
    expect(wrapper.text()).not.toContain('Header mobile')
    expect(wrapper.text()).not.toContain('Tabbar mobile')
  })

  it('renders the mobile slots (header/tabbar) at or below max-width:767px', async () => {
    installMatchMedia(true)
    const wrapper = mount(AppShell, {
      slots: {
        sidebar: '<div>Sidebar</div>',
        topbar: '<div>Topbar</div>',
        default: '<div>Contenuto</div>',
        'mobile-header': '<div>Header mobile</div>',
        'mobile-tabbar': '<div>Tabbar mobile</div>'
      }
    })
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Header mobile')
    expect(wrapper.text()).toContain('Tabbar mobile')
    expect(wrapper.text()).not.toContain('Sidebar')
    expect(wrapper.text()).not.toContain('Topbar')
  })

  it('switches by a real width change event, not a manually flipped switch', async () => {
    installMatchMedia(false)
    const wrapper = mount(AppShell, {
      slots: {
        sidebar: '<div>Sidebar</div>',
        'mobile-header': '<div>Header mobile</div>'
      }
    })
    expect(wrapper.text()).toContain('Sidebar')

    setMatches(true)
    changeHandlers.forEach((handler) => handler({ matches: true } as MediaQueryListEvent))
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Header mobile')
    expect(wrapper.text()).not.toContain('Sidebar')
  })
})

import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { User } from '@/api/auth'
import type { SessionView } from '@/api/sessions'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import ProfileView from './ProfileView.vue'

const fetchSessionsMock = vi.fn()
const revokeSessionMock = vi.fn()
const revokeOtherSessionsMock = vi.fn()
const getTotpStatusMock = vi.fn()
const updateUserMock = vi.fn()
const changePasswordMock = vi.fn()

vi.mock('@/api/sessions', () => ({
  fetchSessions: (...args: unknown[]) => fetchSessionsMock(...args),
  revokeSession: (...args: unknown[]) => revokeSessionMock(...args),
  revokeOtherSessions: (...args: unknown[]) => revokeOtherSessionsMock(...args)
}))

vi.mock('@/api/totp', () => ({
  getTotpStatus: (...args: unknown[]) => getTotpStatusMock(...args)
}))

vi.mock('@/api/users', () => ({
  updateUser: (...args: unknown[]) => updateUserMock(...args),
  changePassword: (...args: unknown[]) => changePasswordMock(...args)
}))

function session(overrides: Partial<SessionView> = {}): SessionView {
  return {
    id: 's1',
    device_label: 'Firefox on Windows',
    last_seen_at: '2026-08-24T12:00:00Z',
    current: false,
    ...overrides
  }
}

const testUser: User = {
  id: 'u1',
  username: 'giovanni',
  display_name: 'Giovanni',
  email: 'giovanni@example.com',
  role: 'admin',
  locale: null
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchSessionsMock.mockResolvedValue([
    session({ id: 'current', current: true, device_label: 'Chrome on macOS' }),
    session()
  ])
  revokeSessionMock.mockResolvedValue(null)
  revokeOtherSessionsMock.mockResolvedValue(null)
  getTotpStatusMock.mockResolvedValue({ enabled: false, pending: false, unused_recovery_codes: 0 })
  updateUserMock.mockResolvedValue({ ...testUser, display_name: 'Nuovo nome' })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
  localStorage.clear()
})

async function mountProfile() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const sessionStore = useSessionStore()
  sessionStore.user = testUser
  sessionStore.initialised = true
  sessionStore.ready = true
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/profile', component: ProfileView },
      { path: '/settings/security/totp', component: { template: '<div />' } }
    ]
  })
  await router.push('/profile')
  await router.isReady()
  wrapper = mount(ProfileView, { global: { plugins: [router, i18n, pinia] }, attachTo: document.body })
  await flushPromises()
  return { wrapper, sessionStore }
}

describe('ProfileView — Profile', () => {
  it('shows the header with role and loads real sessions + TOTP status', async () => {
    const { wrapper } = await mountProfile()

    expect(wrapper.text()).toContain('Giovanni')
    expect(wrapper.text()).toContain('Admin')
    expect(wrapper.text()).toContain('Firefox on Windows')
    expect(wrapper.text()).toContain('Attiva ora')
  })

  it('email is read-only, unlike the mockup\'s inert editable field', async () => {
    const { wrapper } = await mountProfile()

    const emailInput = wrapper.get('input[disabled]')
    expect((emailInput.element as HTMLInputElement).value).toBe('giovanni@example.com')
  })

  it('"Salva modifiche" saves the display name for real via session.updateDisplayName', async () => {
    const { wrapper } = await mountProfile()

    const nameInput = wrapper.get('input:not([disabled])')
    await nameInput.setValue('Nuovo nome')
    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(updateUserMock).toHaveBeenCalledWith('u1', { display_name: 'Nuovo nome' })
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Modifiche salvate.')).toBe(true)
  })

  it('picking an avatar color persists it and marks it selected', async () => {
    const { wrapper } = await mountProfile()

    const swatches = wrapper.findAll('button[aria-label]')
    const blu = swatches.find((b) => b.attributes('aria-label') === 'Blu')
    await blu!.trigger('click')

    expect(blu!.attributes('aria-pressed')).toBe('true')
    expect(localStorage.getItem('keeppix.avatarColor.u1')).toBe('blu')
  })

  it('shows "Attiva" when TOTP is off and links to the real setup flow', async () => {
    const { wrapper } = await mountProfile()

    const link = wrapper.get('a[href="/settings/security/totp"]')
    expect(link.text()).toBe('Attiva')
  })

  it('shows "Gestisci" when TOTP is already enabled', async () => {
    getTotpStatusMock.mockResolvedValue({ enabled: true, pending: false, unused_recovery_codes: 8 })
    const { wrapper } = await mountProfile()

    const link = wrapper.get('a[href="/settings/security/totp"]')
    expect(link.text()).toBe('Gestisci')
  })

  it('"Esci" revokes only that session, real API, no confirmation', async () => {
    const { wrapper } = await mountProfile()

    const logoutBtn = wrapper.findAll('button').find((b) => b.text() === 'Esci')
    await logoutBtn!.trigger('click')
    await flushPromises()

    expect(revokeSessionMock).toHaveBeenCalledWith('s1')
    expect(wrapper.text()).not.toContain('Firefox on Windows')
  })

  it('never shows an "Esci" button on the current session\'s row', async () => {
    const { wrapper } = await mountProfile()

    const rows = wrapper.findAll('li')
    const currentRow = rows.find((r) => r.text().includes('questa sessione'))
    expect(currentRow!.findAll('button')).toHaveLength(0)
  })

  it('"Esci da tutti gli altri dispositivi" asks for confirmation, then revokes and keeps only the current session', async () => {
    const { wrapper } = await mountProfile()

    await wrapper.get('button.text-danger').trigger('click')
    await tick()
    expect(revokeOtherSessionsMock).not.toHaveBeenCalled()

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Esci da tutti'
    )
    confirmBtn?.click()
    await flushPromises()

    expect(revokeOtherSessionsMock).toHaveBeenCalled()
    expect(wrapper.text()).not.toContain('Firefox on Windows')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Uscito dalle altre sessioni.')).toBe(true)
  })

  it('hides "Esci da tutti gli altri dispositivi" when there is nothing to revoke', async () => {
    fetchSessionsMock.mockResolvedValue([session({ id: 'current', current: true })])
    const { wrapper } = await mountProfile()

    expect(wrapper.text()).not.toContain('Esci da tutti gli altri dispositivi')
  })

  it('opens the real "Cambia password" dialog', async () => {
    const { wrapper } = await mountProfile()

    const changeBtn = wrapper.findAll('button').find((b) => b.text() === 'Cambia password')
    await changeBtn!.trigger('click')
    await tick()

    expect(document.body.textContent).toContain('Password attuale')
  })
})

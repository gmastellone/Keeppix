import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

vi.mock('@/api/users', () => ({
  fetchUsers: vi.fn(),
  createUser: vi.fn(),
  updateUser: vi.fn(),
  disableUser: vi.fn(),
  enableUser: vi.fn(),
  changePassword: vi.fn()
}))

vi.mock('@/api/audit', () => ({
  fetchAuditLog: vi.fn()
}))

vi.mock('@/api/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/auth')>()
  return { ...actual, logout: vi.fn() }
})

import UsersView from './UsersView.vue'

const { fetchUsers, createUser, updateUser, disableUser, enableUser, changePassword } =
  await import('@/api/users')
const { fetchAuditLog } = await import('@/api/audit')

const admin = {
  id: 'user-admin',
  username: 'giovanni',
  display_name: 'Giovanni',
  role: 'admin',
  locale: null,
  disabled_at: null
}

const bob = {
  id: 'user-bob',
  username: 'bob',
  display_name: 'Bob',
  role: 'user',
  locale: null,
  disabled_at: null
}

async function mountUsers() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const session = useSessionStore()
  session.user = {
    id: admin.id,
    username: admin.username,
    display_name: admin.display_name,
    email: null,
    role: 'admin',
    locale: null
  }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/users', component: UsersView }
    ]
  })
  await router.push('/users')
  await router.isReady()
  const wrapper = mount(UsersView, { global: { plugins: [router, i18n, pinia] } })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  vi.mocked(fetchUsers).mockResolvedValue([admin])
  vi.mocked(fetchAuditLog).mockResolvedValue([])
  vi.mocked(createUser).mockResolvedValue(bob)
  vi.mocked(updateUser).mockResolvedValue({ ...bob, role: 'admin' })
  vi.mocked(disableUser).mockResolvedValue(null)
  vi.mocked(enableUser).mockResolvedValue(null)
  vi.mocked(changePassword).mockResolvedValue(null)
})

afterEach(() => {
  vi.resetAllMocks()
})

describe('UsersView', () => {
  it('crea un utente dal form, senza chiamare l’API dal test', async () => {
    const wrapper = await mountUsers()

    await wrapper.get('[data-testid="users-username"]').setValue('bob')
    await wrapper.get('[data-testid="users-display-name"]').setValue('Bob')
    await wrapper.get('[data-testid="users-password"]').setValue('bob-password-ok')
    await wrapper.get('[data-testid="users-role"]').setValue('user')
    await wrapper.get('[data-testid="users-create"]').trigger('click')
    await flushPromises()

    expect(createUser).toHaveBeenCalledTimes(1)
    expect(createUser).toHaveBeenCalledWith({
      username: 'bob',
      display_name: 'Bob',
      password: 'bob-password-ok',
      role: 'user'
    })
  })

  it('dopo la creazione Bob compare nell’elenco', async () => {
    vi.mocked(createUser).mockImplementation(async () => {
      vi.mocked(fetchUsers).mockResolvedValue([admin, bob])
      return bob
    })

    const wrapper = await mountUsers()
    await wrapper.get('[data-testid="users-username"]').setValue('bob')
    await wrapper.get('[data-testid="users-display-name"]').setValue('Bob')
    await wrapper.get('[data-testid="users-password"]').setValue('bob-password-ok')
    await wrapper.get('[data-testid="users-create"]').trigger('click')
    await flushPromises()

    expect(wrapper.get('[data-testid="users-list"]').text()).toContain('Bob')
  })

  it('disabilita un utente con un click, non con una chiamata diretta nel test', async () => {
    vi.mocked(fetchUsers).mockResolvedValue([admin, bob])
    const wrapper = await mountUsers()

    await wrapper.get(`[data-testid="users-disable-${bob.id}"]`).trigger('click')
    await flushPromises()

    expect(disableUser).toHaveBeenCalledTimes(1)
    expect(disableUser).toHaveBeenCalledWith(bob.id)
  })

  it('riabilita un utente disabilitato dal pulsante della riga', async () => {
    vi.mocked(fetchUsers).mockResolvedValue([
      admin,
      { ...bob, disabled_at: '2026-08-18T12:00:00Z' }
    ])
    const wrapper = await mountUsers()

    await wrapper.get(`[data-testid="users-enable-${bob.id}"]`).trigger('click')
    await flushPromises()

    expect(enableUser).toHaveBeenCalledWith(bob.id)
  })

  it('cambia il ruolo di una riga dal select', async () => {
    vi.mocked(fetchUsers).mockResolvedValue([admin, bob])
    const wrapper = await mountUsers()

    await wrapper.get(`[data-testid="users-role-${bob.id}"]`).setValue('admin')
    await flushPromises()

    expect(updateUser).toHaveBeenCalledWith(bob.id, { role: 'admin' })
  })

  it('cambia la password dell’utente corrente dal form', async () => {
    const wrapper = await mountUsers()

    await wrapper.get('[data-testid="users-current-password"]').setValue('correct horse battery staple')
    await wrapper.get('[data-testid="users-new-password"]').setValue('new-password-ok')
    await wrapper.get('[data-testid="users-password-save"]').trigger('click')
    await flushPromises()

    expect(changePassword).toHaveBeenCalledWith(
      'correct horse battery staple',
      'new-password-ok'
    )
  })
})

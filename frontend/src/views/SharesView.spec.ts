import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

vi.mock('@/api/shares', () => ({
  fetchShareLinks: vi.fn(),
  revokeShareLink: vi.fn()
}))

vi.mock('@/api/permissions', () => ({
  fetchPermissions: vi.fn(),
  grantPermission: vi.fn(),
  explainPermission: vi.fn(),
  revokePermission: vi.fn()
}))

vi.mock('@/api/folders', () => ({
  fetchTree: vi.fn()
}))

vi.mock('@/api/users', () => ({
  fetchUsers: vi.fn()
}))

vi.mock('@/api/groups', () => ({
  fetchGroups: vi.fn()
}))

import SharesView from './SharesView.vue'

const { fetchShareLinks } = await import('@/api/shares')
const { fetchPermissions, grantPermission, explainPermission } = await import('@/api/permissions')
const { fetchTree } = await import('@/api/folders')
const { fetchUsers } = await import('@/api/users')
const { fetchGroups } = await import('@/api/groups')

const folder = {
  id: 'folder-vacanze',
  library_id: 'lib',
  parent_id: null,
  name: 'Vacanze',
  depth: 0
}

const bob = {
  id: 'user-bob',
  username: 'bob',
  display_name: 'Bob',
  role: 'user',
  locale: null,
  disabled_at: null
}

const famiglia = {
  id: 'group-famiglia',
  name: 'Famiglia',
  member_count: 3,
  created_at: '2026-01-01T00:00:00Z'
}

async function mountShares() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/shares', component: SharesView }
    ]
  })
  await router.push('/shares')
  await router.isReady()
  const wrapper = mount(SharesView, { global: { plugins: [router, i18n, pinia] } })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  vi.mocked(fetchShareLinks).mockResolvedValue([])
  vi.mocked(fetchTree).mockResolvedValue([folder])
  vi.mocked(fetchUsers).mockResolvedValue([bob])
  vi.mocked(fetchGroups).mockResolvedValue([famiglia])
  vi.mocked(fetchPermissions).mockResolvedValue([])
  vi.mocked(grantPermission).mockResolvedValue({ id: 'grant-1' })
  vi.mocked(explainPermission).mockResolvedValue({ granted: false, chain: [] })
})

afterEach(() => {
  vi.resetAllMocks()
})

describe('SharesView', () => {
  it('condivide una cartella con una persona dal form, senza chiamare l’API dal test', async () => {
    const wrapper = await mountShares()

    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject-type"]').setValue('user')
    await wrapper.get('[data-testid="shares-subject"]').setValue(bob.id)
    await wrapper.get('[data-testid="shares-role"]').setValue('viewer')
    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(grantPermission).toHaveBeenCalledTimes(1)
    expect(grantPermission).toHaveBeenCalledWith({
      subject_type: 'user',
      subject_id: bob.id,
      object_type: 'folder',
      object_id: folder.id,
      role: 'viewer',
      inherit: true
    })
  })

  it('dopo il grant, Bob compare nell’elenco dei permessi diretti', async () => {
    vi.mocked(grantPermission).mockImplementation(async () => {
      vi.mocked(fetchPermissions).mockResolvedValue([
        {
          id: 'grant-1',
          subject_type: 'user',
          subject_id: bob.id,
          role: 'viewer',
          inherit: true,
          inherited: false
        }
      ])
      return { id: 'grant-1' }
    })

    const wrapper = await mountShares()
    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject"]').setValue(bob.id)
    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(wrapper.get('[data-testid="shares-grants"]').text()).toContain('Bob')
    expect(wrapper.get('[data-testid="shares-grants"]').text()).toContain('viewer')
  })

  it('mostra la catena di explain dopo un click, non dopo una chiamata diretta nel test', async () => {
    vi.mocked(explainPermission).mockResolvedValue({
      granted: true,
      chain: [
        {
          subject_type: 'group',
          subject_name: 'Famiglia',
          role: 'viewer',
          granted_on_type: 'folder',
          granted_on_name: '/Foto/Vacanze',
          inherited_in: '/Foto/Vacanze/2024/Grecia'
        }
      ]
    })

    const wrapper = await mountShares()
    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-explain-user"]').setValue(bob.id)
    await wrapper.get('[data-testid="shares-explain"]').trigger('click')
    await flushPromises()

    expect(explainPermission).toHaveBeenCalledWith('folder', folder.id, bob.id)
    const chain = wrapper.get('[data-testid="shares-explain-chain"]').text()
    expect(chain).toContain('Famiglia')
    expect(chain).toContain('viewer')
    expect(chain).toContain('/Foto/Vacanze')
    expect(chain).toContain('/Foto/Vacanze/2024/Grecia')
    expect(chain).not.toMatch(
      /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i
    )
  })

  it('condivide una cartella con un gruppo scegliendo gruppo nel form', async () => {
    const wrapper = await mountShares()
    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject-type"]').setValue('group')
    await flushPromises()
    await wrapper.get('[data-testid="shares-subject"]').setValue(famiglia.id)
    await wrapper.get('[data-testid="shares-role"]').setValue('editor')
    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(grantPermission).toHaveBeenCalledWith({
      subject_type: 'group',
      subject_id: famiglia.id,
      object_type: 'folder',
      object_id: folder.id,
      role: 'editor',
      inherit: true
    })
  })
})

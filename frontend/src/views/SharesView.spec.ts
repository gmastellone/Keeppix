import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

vi.mock('@/api/shares', () => ({
  fetchShareLinks: vi.fn(),
  revokeShareLink: vi.fn()
}))

vi.mock('@/api/permissions', () => ({
  fetchPermissions: vi.fn(),
  grantPermission: vi.fn(),
  explainPermission: vi.fn(),
  revokePermission: vi.fn(),
  fetchSharedWithMe: vi.fn()
}))

vi.mock('@/api/folders', () => ({
  fetchTree: vi.fn(),
  fetchAllFolders: vi.fn()
}))

vi.mock('@/api/albums', () => ({
  fetchAlbums: vi.fn(),
  fetchAlbum: vi.fn()
}))

vi.mock('@/api/library', () => ({
  runSearch: vi.fn()
}))

vi.mock('@/api/users', () => ({
  fetchUsers: vi.fn()
}))

vi.mock('@/api/groups', () => ({
  fetchGroups: vi.fn()
}))

import SharesView from './SharesView.vue'

const { fetchShareLinks } = await import('@/api/shares')
const { fetchPermissions, grantPermission, explainPermission, fetchSharedWithMe } = await import('@/api/permissions')
const { fetchTree, fetchAllFolders } = await import('@/api/folders')
const { fetchAlbums, fetchAlbum } = await import('@/api/albums')
const { runSearch } = await import('@/api/library')
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

const admin = { id: 'user-admin', username: 'admin', display_name: 'Admin', email: null, role: 'admin' as const, locale: null }

async function mountShares(asAdmin = false) {
  const pinia = createPinia()
  setActivePinia(pinia)
  if (asAdmin) useSessionStore().user = admin
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/shares', component: SharesView },
      { path: '/folders', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } }
    ]
  })
  await router.push('/shares')
  await router.isReady()
  const wrapper = mount(SharesView, { global: { plugins: [router, i18n, pinia] } })
  await flushPromises()
  return { wrapper, router }
}

beforeEach(() => {
  i18n.global.locale.value = 'it'
  vi.mocked(fetchShareLinks).mockResolvedValue([])
  vi.mocked(fetchTree).mockResolvedValue([folder])
  vi.mocked(fetchAllFolders).mockResolvedValue([folder])
  vi.mocked(fetchAlbums).mockResolvedValue([])
  vi.mocked(fetchAlbum).mockResolvedValue({ id: 'x', name: '', assets: [] })
  vi.mocked(runSearch).mockResolvedValue({ assets: [] })
  vi.mocked(fetchUsers).mockResolvedValue([bob])
  vi.mocked(fetchGroups).mockResolvedValue([famiglia])
  vi.mocked(fetchPermissions).mockResolvedValue([])
  vi.mocked(grantPermission).mockResolvedValue({ id: 'grant-1' })
  vi.mocked(explainPermission).mockResolvedValue({ granted: false, chain: [] })
  vi.mocked(fetchSharedWithMe).mockResolvedValue([])
})

afterEach(() => {
  vi.resetAllMocks()
})

/** Le sezioni "Persone"/"Invita" (§29) sono riservate agli admin — `GET
 * /users`/`GET /groups` sono `AdminAuth` sul backend reale, verificato
 * in `crates/keeppix-api/src/routes/users.rs`/`groups.rs` (Task 11
 * 1/N). I test sul form di invito impersonano un admin. */
async function mountSharesAsAdmin() {
  return mountShares(true)
}

describe('SharesView — "Invita" (form di concessione, admin-only)', () => {
  it('condivide una cartella con una persona dal form, senza chiamare l’API dal test', async () => {
    const { wrapper } = await mountSharesAsAdmin()
    await wrapper.findAll('button').find((b) => b.text() === 'Invita')!.trigger('click')
    await flushPromises()

    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject-type"]').setValue('user')
    await wrapper.get('[data-testid="shares-subject"]').setValue(bob.id)
    await flushPromises()

    expect(grantPermission).not.toHaveBeenCalled()

    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(grantPermission).toHaveBeenCalledWith({
      subject_type: 'user',
      subject_id: bob.id,
      object_type: 'folder',
      object_id: folder.id,
      role: 'viewer',
      inherit: true
    })
  })

  it('dopo il grant, Bob compare nella sezione Persone', async () => {
    vi.mocked(fetchPermissions).mockResolvedValue([])
    const { wrapper } = await mountSharesAsAdmin()
    await wrapper.findAll('button').find((b) => b.text() === 'Invita')!.trigger('click')
    await flushPromises()

    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject"]').setValue(bob.id)
    await flushPromises()

    vi.mocked(fetchPermissions).mockResolvedValue([
      { id: 'grant-1', subject_type: 'user', subject_id: bob.id, role: 'viewer', inherit: true, inherited: false }
    ])
    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Bob')
  })

  it('mostra la catena di explain dopo un click, non dopo una chiamata diretta nel test', async () => {
    const { wrapper } = await mountSharesAsAdmin()
    await wrapper.findAll('button').find((b) => b.text() === 'Invita')!.trigger('click')
    await flushPromises()

    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await flushPromises()

    vi.mocked(explainPermission).mockResolvedValue({
      granted: true,
      chain: [{ subject_type: 'user', subject_name: 'Bob', role: 'viewer', granted_on_type: 'folder', granted_on_name: 'Vacanze' }]
    })
    await wrapper.get('[data-testid="shares-explain-user"]').setValue(bob.id)
    await wrapper.get('[data-testid="shares-explain"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Vacanze')
  })

  it('condivide una cartella con un gruppo scegliendo gruppo nel form', async () => {
    const { wrapper } = await mountSharesAsAdmin()
    await wrapper.findAll('button').find((b) => b.text() === 'Invita')!.trigger('click')
    await flushPromises()

    await wrapper.get('[data-testid="shares-folder"]').setValue(folder.id)
    await wrapper.get('[data-testid="shares-subject-type"]').setValue('group')
    await flushPromises()
    await wrapper.get('[data-testid="shares-subject"]').setValue(famiglia.id)
    await wrapper.get('[data-testid="shares-grant"]').trigger('click')
    await flushPromises()

    expect(grantPermission).toHaveBeenCalledWith(
      expect.objectContaining({ subject_type: 'group', subject_id: famiglia.id })
    )
  })
})

describe('SharesView — §29, la pagina reale', () => {
  it('non-admin: la sezione Persone e "Invita" sono nascoste, il resto della pagina funziona', async () => {
    const { wrapper } = await mountShares()

    expect(fetchUsers).not.toHaveBeenCalled()
    expect(wrapper.text()).not.toContain('Invita')
    expect(wrapper.text()).toContain('Le mie condivisioni')
  })

  it('le due schede: "Condivisi con me" mostra lo stato vuoto quando non c\'è nulla', async () => {
    const { wrapper } = await mountShares()

    await wrapper.findAll('button').find((b) => b.text() === 'Condivisi con me')!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Niente condiviso con te')
  })

  it('"Condivisi con me" mostra gli elementi reali da fetchSharedWithMe, non uno stub', async () => {
    vi.mocked(fetchSharedWithMe).mockResolvedValue([
      { object_type: 'album', object_id: 'al1', name: 'Weekend in montagna', owner_name: 'Mich', role: 'editor', item_count: 63 }
    ])
    const { wrapper } = await mountShares()

    await wrapper.findAll('button').find((b) => b.text() === 'Condivisi con me')!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Weekend in montagna')
    expect(wrapper.text()).toContain('da Mich')
    expect(wrapper.text()).toContain('63 elementi')
  })

  it('un link pubblico mostra il nome risolto dell\'oggetto e la riga riassuntiva reale', async () => {
    vi.mocked(fetchAlbums).mockResolvedValue([{ id: 'al1', name: 'Migliori scatti 2026', cover_hash: null, created_at: '' }])
    vi.mocked(fetchShareLinks).mockResolvedValue([
      {
        id: 'link-1',
        object_type: 'album',
        object_id: 'al1',
        has_password: true,
        expires_at: null,
        max_views: null,
        view_count: 4,
        allow_download: true,
        allow_original: false,
        allow_upload: false,
        hide_metadata: true,
        revoked_at: null,
        last_accessed_at: null,
        created_at: '2026-01-01T00:00:00Z',
        item_count: 84
      }
    ])
    const { wrapper } = await mountShares()

    expect(wrapper.text()).toContain('Migliori scatti 2026')
    expect(wrapper.text()).toContain('password attiva')
    expect(wrapper.text()).toContain('84 elementi')
    expect(wrapper.text()).toContain('nessuna scadenza')
  })

  it('"Copia" non compare per un link caricato dalla lista — il token non è mai ri-esposto da GET /share/links', async () => {
    vi.mocked(fetchShareLinks).mockResolvedValue([
      {
        id: 'link-1',
        object_type: 'folder',
        object_id: folder.id,
        has_password: false,
        expires_at: null,
        max_views: null,
        view_count: 0,
        allow_download: true,
        allow_original: false,
        allow_upload: false,
        hide_metadata: true,
        revoked_at: null,
        last_accessed_at: null,
        created_at: '2026-01-01T00:00:00Z',
        item_count: 3
      }
    ])
    const { wrapper } = await mountShares()

    expect(wrapper.findAll('button').some((b) => b.text() === 'Copia')).toBe(false)
    expect(wrapper.findAll('button').some((b) => b.text() === 'Revoca')).toBe(true)
  })

  it('revocare un link lo rimuove dalla lista', async () => {
    vi.mocked(fetchShareLinks).mockResolvedValue([
      {
        id: 'link-1',
        object_type: 'folder',
        object_id: folder.id,
        has_password: false,
        expires_at: null,
        max_views: null,
        view_count: 0,
        allow_download: true,
        allow_original: false,
        allow_upload: false,
        hide_metadata: true,
        revoked_at: null,
        last_accessed_at: null,
        created_at: '2026-01-01T00:00:00Z',
        item_count: 3
      }
    ])
    const { wrapper } = await mountShares()
    expect(wrapper.text()).toContain('Vacanze')

    await wrapper.findAll('button').find((b) => b.text() === 'Revoca')!.trigger('click')
    await flushPromises()

    const { revokeShareLink } = await import('@/api/shares')
    expect(revokeShareLink).toHaveBeenCalledWith('link-1')
    expect(wrapper.text()).not.toContain('password attiva')
  })

  it('una cartella condivisa via link compare come card cliccabile in "Cartelle e album condivisi", verso /folders', async () => {
    vi.mocked(fetchShareLinks).mockResolvedValue([
      {
        id: 'link-1',
        object_type: 'folder',
        object_id: folder.id,
        has_password: false,
        expires_at: null,
        max_views: null,
        view_count: 0,
        allow_download: true,
        allow_original: false,
        allow_upload: false,
        hide_metadata: true,
        revoked_at: null,
        last_accessed_at: null,
        created_at: '2026-01-01T00:00:00Z',
        item_count: 3
      }
    ])
    vi.mocked(runSearch).mockResolvedValue({ assets: [{ id: 'a' }, { id: 'b' }, { id: 'c' }] as never })
    const { wrapper, router } = await mountShares()
    await flushPromises()

    const card = wrapper.findAll('button').find((b) => b.text().includes('Vacanze') && b.text().includes('condiviso'))
    expect(card).toBeTruthy()
    await card?.trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/folders')
  })
})

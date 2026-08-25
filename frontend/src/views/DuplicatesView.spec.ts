import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import type { DuplicateGroup } from '@/api/library'
import { i18n } from '@/i18n'
import ErrorState from '@/components/ui/ErrorState.vue'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import DuplicatesView from './DuplicatesView.vue'

const fetchDuplicatesMock = vi.fn()
const fetchDuplicateMembersMock = vi.fn()
const resolveDuplicateGroupMock = vi.fn()

vi.mock('@/api/library', () => ({
  fetchDuplicates: (...args: unknown[]) => fetchDuplicatesMock(...args),
  fetchDuplicateMembers: (...args: unknown[]) => fetchDuplicateMembersMock(...args),
  resolveDuplicateGroup: (...args: unknown[]) => resolveDuplicateGroupMock(...args)
}))

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

function group(overrides: Partial<DuplicateGroup> = {}): DuplicateGroup {
  return {
    content_hash: 'h1',
    count: 2,
    size_bytes: 3 * 1024 * 1024,
    reclaimable_bytes: 3 * 1024 * 1024,
    ...overrides
  }
}

function member(id: string, filename: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename,
    content_hash: 'ab'.repeat(32),
    size_bytes: 3 * 1024 * 1024,
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

beforeEach(() => {
  i18n.global.locale.value = 'it'
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  fetchDuplicatesMock.mockResolvedValue([])
  fetchDuplicateMembersMock.mockResolvedValue([])
  resolveDuplicateGroupMock.mockResolvedValue({ resolved: 1 })
})

afterEach(() => {
  vi.clearAllMocks()
})

async function mountDuplicates() {
  const wrapper = mount(DuplicatesView, { global: { plugins: [i18n] } })
  await flushPromises()
  return wrapper
}

describe('DuplicatesView — §46 Duplicati', () => {
  it('shows the documented empty state, with the content-hash explanation', async () => {
    const wrapper = await mountDuplicates()

    expect(wrapper.text()).toContain('Nessun duplicato trovato')
    expect(wrapper.text()).toContain("confronta l'hash del contenuto")
  })

  it('renders each group with real thumbnails from fetchDuplicateMembers, defaulting to the first member as "keep"', async () => {
    fetchDuplicatesMock.mockResolvedValue([group()])
    fetchDuplicateMembersMock.mockResolvedValue([member('a', 'photo.jpg'), member('b', 'photo (1).jpg')])

    const wrapper = await mountDuplicates()

    expect(wrapper.text()).toContain('2 file identici (stesso hash del contenuto)')
    const buttons = wrapper.findAll('[role="button"][aria-pressed]')
    expect(buttons).toHaveLength(2)
    expect(buttons[0]?.attributes('aria-pressed')).toBe('true')
    expect(buttons[1]?.attributes('aria-pressed')).toBe('false')
  })

  it('clicking another thumbnail moves the "keep" choice to it', async () => {
    fetchDuplicatesMock.mockResolvedValue([group()])
    fetchDuplicateMembersMock.mockResolvedValue([member('a', 'photo.jpg'), member('b', 'photo (1).jpg')])
    const wrapper = await mountDuplicates()

    const buttons = wrapper.findAll('[role="button"][aria-pressed]')
    await buttons[1]!.trigger('click')

    expect(wrapper.findAll('[role="button"][aria-pressed]')[0]?.attributes('aria-pressed')).toBe('false')
    expect(wrapper.findAll('[role="button"][aria-pressed]')[1]?.attributes('aria-pressed')).toBe('true')
  })

  it('"Risolvi gruppo" opens the 3-option delete dialog with the right count, and resolving applies it for real', async () => {
    fetchDuplicatesMock.mockResolvedValue([group({ count: 3 })])
    fetchDuplicateMembersMock.mockResolvedValue([member('a', 'photo.jpg'), member('b', 'photo (1).jpg'), member('c', 'photo (2).jpg')])
    resolveDuplicateGroupMock.mockResolvedValue({ resolved: 2 })
    const wrapper = await mountDuplicates()

    const resolveBtn = wrapper.findAll('button').find((b) => b.text() === 'Risolvi gruppo')
    await resolveBtn!.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('Eliminare 2 copie duplicate?')
    const trashOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Sposta nel cestino')
    )
    trashOption?.click()
    await flushPromises()

    expect(resolveDuplicateGroupMock).toHaveBeenCalledWith('h1', 'a', 'moved_to_trash')
    expect(wrapper.text()).not.toContain('2 file identici')
    const toast = useToastStore()
    expect(toast.toasts.some((entry) => entry.message.includes('2 copie rimosse, mantenuta photo.jpg'))).toBe(true)
  })

  it('"Ignora" removes the group from view without touching any file, with its own toast', async () => {
    fetchDuplicatesMock.mockResolvedValue([group()])
    fetchDuplicateMembersMock.mockResolvedValue([member('a', 'photo.jpg'), member('b', 'photo (1).jpg')])
    const wrapper = await mountDuplicates()

    const ignoreBtn = wrapper.findAll('button').find((b) => b.text() === 'Ignora')
    await ignoreBtn!.trigger('click')
    await flushPromises()

    expect(resolveDuplicateGroupMock).not.toHaveBeenCalled()
    expect(wrapper.text()).toContain('Nessun duplicato trovato')
    const toast = useToastStore()
    expect(toast.toasts.some((entry) => entry.message.includes('non verrà più segnalato'))).toBe(true)
  })

  it('shows a full-view ErrorState on load failure', async () => {
    fetchDuplicatesMock.mockRejectedValue(new Error('boom'))

    const wrapper = await mountDuplicates()

    expect(wrapper.findComponent(ErrorState).exists()).toBe(true)
  })
})

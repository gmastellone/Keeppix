import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import ReviewView from './ReviewView.vue'

const fetchTagProposalsMock = vi.fn()
const fetchTagsMock = vi.fn()
const confirmTagProposalMock = vi.fn()
const rejectTagProposalMock = vi.fn()
const confirmAllTagProposalsMock = vi.fn()
const rejectAllTagProposalsMock = vi.fn()
const fetchAssetMock = vi.fn()

vi.mock('@/api/tags', () => ({
  fetchTagProposals: (...args: unknown[]) => fetchTagProposalsMock(...args),
  fetchTags: (...args: unknown[]) => fetchTagsMock(...args),
  confirmTagProposal: (...args: unknown[]) => confirmTagProposalMock(...args),
  rejectTagProposal: (...args: unknown[]) => rejectTagProposalMock(...args),
  confirmAllTagProposals: (...args: unknown[]) => confirmAllTagProposalsMock(...args),
  rejectAllTagProposals: (...args: unknown[]) => rejectAllTagProposalsMock(...args)
}))

vi.mock('@/api/timeline', () => ({
  fetchAsset: (...args: unknown[]) => fetchAssetMock(...args)
}))

function proposal(overrides: Partial<{
  asset_id: string
  tag_id: string
  tag_name: string
  score?: number
  filename: string
  taken_at_utc?: string
}> = {}) {
  return {
    asset_id: 'a1',
    tag_id: 't1',
    tag_name: 'Tramonti',
    filename: 'a1.jpg',
    ...overrides
  }
}

function tag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 't1',
    name: 'Tramonti',
    kind: 'tag',
    parent_id: null,
    color: '#e0578a',
    threshold: 0.75,
    assignment_count: 0,
    ...overrides
  }
}

function asset(overrides: Partial<TimelineAsset> = {}): TimelineAsset {
  return {
    id: 'a1',
    folder_id: 'f1',
    filename: 'a1.jpg',
    content_hash: 'hash1',
    size_bytes: 100,
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
    faces: [],
    ...overrides
  } as TimelineAsset
}

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchTagsMock.mockResolvedValue([tag()])
  fetchAssetMock.mockResolvedValue(asset())
  confirmTagProposalMock.mockResolvedValue(null)
  rejectTagProposalMock.mockResolvedValue(null)
  confirmAllTagProposalsMock.mockResolvedValue(null)
  rejectAllTagProposalsMock.mockResolvedValue(null)
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountReview() {
  const pinia = createPinia()
  setActivePinia(pinia)
  wrapper = mount(ReviewView, { global: { plugins: [i18n, pinia] }, attachTo: document.body })
  await flushPromises()
  return { wrapper }
}

describe('ReviewView — §56 Revisione (coda tag)', () => {
  it('groups proposals by tag and shows the total count', async () => {
    fetchTagProposalsMock.mockResolvedValue([
      proposal({ asset_id: 'a1', tag_id: 't1', tag_name: 'Tramonti' }),
      proposal({ asset_id: 'a2', tag_id: 't1', tag_name: 'Tramonti' })
    ])
    const { wrapper } = await mountReview()

    expect(wrapper.text()).toContain('«Tramonti»')
    expect(wrapper.text()).toContain('2 proposte')
  })

  it('shows the empty state when there are no pending proposals', async () => {
    fetchTagProposalsMock.mockResolvedValue([])
    const { wrapper } = await mountReview()

    expect(wrapper.text()).toContain('Nessun suggerimento in attesa')
  })

  it('renders a real thumbnail via fetchAsset content_hash', async () => {
    fetchTagProposalsMock.mockResolvedValue([proposal()])
    fetchAssetMock.mockResolvedValue(asset({ id: 'a1', content_hash: 'realhash' }))
    const { wrapper } = await mountReview()

    const img = wrapper.get('img')
    expect(img.attributes('src')).toContain('realhash')
  })

  it('confirming a single proposal calls confirmTagProposal and removes it', async () => {
    fetchTagProposalsMock.mockResolvedValue([proposal({ asset_id: 'a1', tag_id: 't1' })])
    const { wrapper } = await mountReview()

    const confirmBtn = wrapper.get('[aria-label="Conferma"]')
    await confirmBtn.trigger('click')
    await flushPromises()

    expect(confirmTagProposalMock).toHaveBeenCalledWith('t1', 'a1')
    expect(wrapper.text()).toContain('Nessun suggerimento in attesa')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Tag confermato.')).toBe(true)
  })

  it('rejecting a single proposal calls rejectTagProposal', async () => {
    fetchTagProposalsMock.mockResolvedValue([proposal({ asset_id: 'a1', tag_id: 't1' })])
    const { wrapper } = await mountReview()

    await wrapper.get('[aria-label="Rifiuta"]').trigger('click')
    await flushPromises()

    expect(rejectTagProposalMock).toHaveBeenCalledWith('t1', 'a1')
  })

  it('"Conferma tutte" confirms the whole group in one call', async () => {
    fetchTagProposalsMock.mockResolvedValue([
      proposal({ asset_id: 'a1', tag_id: 't1' }),
      proposal({ asset_id: 'a2', tag_id: 't1' })
    ])
    const { wrapper } = await mountReview()

    const confirmAllBtn = wrapper.findAll('button').find((b) => b.text() === 'Conferma tutte')
    await confirmAllBtn!.trigger('click')
    await flushPromises()

    expect(confirmAllTagProposalsMock).toHaveBeenCalledWith('t1')
    expect(wrapper.text()).toContain('Nessun suggerimento in attesa')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === '2 proposte confermate.')).toBe(true)
  })

  it('"Rifiuta tutte" rejects the whole group in one call', async () => {
    fetchTagProposalsMock.mockResolvedValue([proposal({ asset_id: 'a1', tag_id: 't1' })])
    const { wrapper } = await mountReview()

    const rejectAllBtn = wrapper.findAll('button').find((b) => b.text() === 'Rifiuta tutte')
    await rejectAllBtn!.trigger('click')
    await flushPromises()

    expect(rejectAllTagProposalsMock).toHaveBeenCalledWith('t1')
  })

  it('keeps other groups when one group is fully decided', async () => {
    fetchTagProposalsMock.mockResolvedValue([
      proposal({ asset_id: 'a1', tag_id: 't1', tag_name: 'Tramonti' }),
      proposal({ asset_id: 'a2', tag_id: 't2', tag_name: 'Montagne' })
    ])
    fetchTagsMock.mockResolvedValue([tag({ id: 't1', name: 'Tramonti' }), tag({ id: 't2', name: 'Montagne' })])
    const { wrapper } = await mountReview()

    const confirmAllButtons = wrapper.findAll('button').filter((b) => b.text() === 'Conferma tutte')
    await confirmAllButtons[0].trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('«Montagne»')
    expect(wrapper.text()).not.toContain('«Tramonti»')
  })
})

import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import ProblemsView from './ProblemsView.vue'

vi.mock('@/api/library', () => ({
  fetchProblems: vi.fn(),
  fetchDuplicates: vi.fn()
}))

const { fetchProblems, fetchDuplicates } = await import('@/api/library')

afterEach(() => vi.resetAllMocks())

describe('ProblemsView', () => {
  it('shows an error and retry when loading fails', async () => {
    vi.mocked(fetchProblems).mockRejectedValue(new Error('offline'))
    const wrapper = mount(ProblemsView, { global: { plugins: [i18n] } })
    await flushPromises()
    expect(wrapper.text()).toContain('An unexpected error occurred.')
    expect(wrapper.text()).toContain('Retry')

    vi.mocked(fetchProblems).mockResolvedValue({
      offline_libraries: [],
      failed_jobs: [],
      error_assets: []
    })
    vi.mocked(fetchDuplicates).mockResolvedValue([])
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('Nothing to report.')
  })
})

import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import { i18n } from '@/i18n'

import ScanStep from './ScanStep.vue'

vi.mock('@/api/libraries', () => ({
  startLibraryScan: vi.fn(),
  fetchLibraryScanStatus: vi.fn()
}))

const { startLibraryScan, fetchLibraryScanStatus } = await import('@/api/libraries')

const LIBRARY_ID = 'lib-1'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.resetAllMocks()
  vi.useRealTimers()
})

async function mountStep() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div>home</div>' } }]
  })
  await router.push('/')
  await router.isReady()

  const wrapper = mount(ScanStep, {
    global: { plugins: [i18n, router] },
    props: { libraryId: LIBRARY_ID }
  })
  await flushPromises()
  return { wrapper, router }
}

describe('ScanStep', () => {
  it('progress updates', async () => {
    vi.mocked(startLibraryScan).mockResolvedValue({ library_id: LIBRARY_ID, status: 'accepted' })
    vi.mocked(fetchLibraryScanStatus)
      .mockResolvedValueOnce({
        library_id: LIBRARY_ID,
        library_status: 'active',
        phase: 'discovering',
        asset_count: 3,
        job_status: 'running',
        last_error: null,
        eta_seconds: null,
        last_scan_at: null
      })
      .mockResolvedValueOnce({
        library_id: LIBRARY_ID,
        library_status: 'active',
        phase: 'discovering',
        asset_count: 12,
        job_status: 'running',
        last_error: null,
        eta_seconds: null,
        last_scan_at: null
      })

    const { wrapper } = await mountStep()
    expect(wrapper.text()).toContain('3 photos indexed')

    await vi.advanceTimersByTimeAsync(2000)
    await flushPromises()

    expect(fetchLibraryScanStatus).toHaveBeenCalledTimes(2)
    expect(wrapper.text()).toContain('12 photos indexed')
  })

  it('network error during scan does not blank the page', async () => {
    vi.mocked(startLibraryScan).mockResolvedValue({ library_id: LIBRARY_ID, status: 'accepted' })
    vi.mocked(fetchLibraryScanStatus)
      .mockResolvedValueOnce({
        library_id: LIBRARY_ID,
        library_status: 'active',
        phase: 'discovering',
        asset_count: 5,
        job_status: 'running',
        last_error: null,
        eta_seconds: null,
        last_scan_at: null
      })
      .mockRejectedValueOnce(
        new ApiProblem('keeppix/service-unavailable', 'Service unavailable', 503)
      )
      .mockRejectedValueOnce(
        new ApiProblem('keeppix/unauthenticated', 'Unauthenticated', 401)
      )

    const { wrapper } = await mountStep()
    expect(wrapper.text()).toContain('5 photos indexed')
    expect(wrapper.text()).toContain('Discovering files')

    await vi.advanceTimersByTimeAsync(2000)
    await flushPromises()

    expect(wrapper.text()).toContain('5 photos indexed')
    expect(wrapper.text()).toContain('The server is unreachable')
    expect(wrapper.text()).not.toBe('')

    await vi.advanceTimersByTimeAsync(2000)
    await flushPromises()

    expect(wrapper.text()).toContain('5 photos indexed')
    expect(wrapper.text()).toContain('Your session expired')
    expect(wrapper.text()).toContain('Discovering files')
  })
})

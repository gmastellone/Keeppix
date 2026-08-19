import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import WebdavSetupView from './WebdavSetupView.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

afterEach(() => {
  apiFetch.mockReset()
  vi.useRealTimers()
})

function mountView() {
  return mount(WebdavSetupView, { global: { plugins: [i18n] } })
}

describe('WebdavSetupView', () => {
  it('secret_is_shown_only_after_generating_not_before', async () => {
    apiFetch.mockImplementation((_path: string, init?: RequestInit) => {
      if (init?.method === 'POST') {
        return Promise.resolve({
          id: 'ap-1',
          label: 'MacBook',
          secret: 'kpx_7Fq2xxxxxxxx9dLm',
          created_at: '2026-08-19T00:00:00Z',
          last_used_at: null
        })
      }
      return Promise.resolve([])
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).not.toContain('kpx_7Fq2xxxxxxxx9dLm')

    await wrapper.get('[name="app-password-label"]').setValue('MacBook')
    await wrapper.get('[data-action="generate-app-password"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/users/me/app-passwords', {
      method: 'POST',
      body: JSON.stringify({ label: 'MacBook' })
    })
    expect(wrapper.text()).toContain('kpx_7Fq2xxxxxxxx9dLm')
  })

  it('live_indicator_shows_connected_when_last_used_at_becomes_non_null', async () => {
    vi.useFakeTimers()
    let listCalls = 0
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (init?.method === 'POST') {
        return Promise.resolve({
          id: 'ap-1',
          label: 'MacBook',
          secret: 'kpx_7Fq2xxxxxxxx9dLm',
          created_at: '2026-08-19T00:00:00Z',
          last_used_at: null
        })
      }
      if (path === '/api/v1/users/me/app-passwords') {
        listCalls += 1
        if (listCalls === 1) return Promise.resolve([])
        return Promise.resolve([
          listCalls < 3
            ? { id: 'ap-1', label: 'MacBook', created_at: '2026-08-19T00:00:00Z', last_used_at: null }
            : {
                id: 'ap-1',
                label: 'MacBook',
                created_at: '2026-08-19T00:00:00Z',
                last_used_at: '2026-08-19T10:00:00Z'
              }
        ])
      }
      // (call 1 = mount, call 2 = first poll tick, call 3 = second poll tick)
      throw new Error(`unexpected ${path}`)
    })

    const wrapper = mountView()
    await flushPromises()

    await wrapper.get('[name="app-password-label"]').setValue('MacBook')
    await wrapper.get('[data-action="generate-app-password"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).not.toContain('Connected')

    await vi.advanceTimersByTimeAsync(3000)
    await flushPromises()
    expect(wrapper.text()).not.toContain('Connected')

    await vi.advanceTimersByTimeAsync(3000)
    await flushPromises()

    expect(wrapper.text()).toContain('Connected')
  })
})

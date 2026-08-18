import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import ProblemsView from './ProblemsView.vue'

vi.mock('@/api/library', () => ({
  fetchProblems: vi.fn(),
  fetchDuplicates: vi.fn()
}))

const { fetchProblems, fetchDuplicates } = await import('@/api/library')

afterEach(() => vi.resetAllMocks())

async function mountProblems() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const session = useSessionStore()
  session.user = {
    id: 'user-admin',
    username: 'giovanni',
    display_name: 'Giovanni',
    email: null,
    role: 'admin',
    locale: null
  }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/login', component: { template: '<div />' } },
      { path: '/problems', component: ProblemsView }
    ]
  })
  await router.push('/problems')
  await router.isReady()
  const wrapper = mount(ProblemsView, { global: { plugins: [router, i18n, pinia] } })
  return { wrapper, router, session }
}

describe('ProblemsView', () => {
  it('shows an error and retry when loading fails', async () => {
    vi.mocked(fetchProblems).mockRejectedValue(new Error('offline'))
    const { wrapper } = await mountProblems()
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

  it('su sessione scaduta (401) rimanda al login invece di mostrare un errore generico', async () => {
    vi.mocked(fetchProblems).mockRejectedValue(
      new ApiProblem('keeppix/unauthenticated', 'Authentication required', 401)
    )
    const { wrapper, router, session } = await mountProblems()
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/login')
    expect(session.user).toBeNull()
    expect(wrapper.text()).not.toContain('An unexpected error occurred.')
  })
})

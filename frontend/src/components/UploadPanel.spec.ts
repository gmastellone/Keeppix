import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

vi.mock('@/api/upload', () => ({
  checkHashes: vi.fn(),
  createSession: vi.fn(),
  headSession: vi.fn(),
  patchChunk: vi.fn(),
  hashBytes: vi.fn(),
  hashFile: vi.fn()
}))

const { useUploadStore } = await import('@/stores/upload')
const UploadPanel = (await import('./UploadPanel.vue')).default

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  localStorage.clear()
})

describe('pannello di upload persistente — UploadPanel.vue', () => {
  it('renders_the_specific_session_error_instead_of_the_generic_failed_label', () => {
    const store = useUploadStore()
    store.sessions.push({
      id: 'session-err',
      filename: 'e.jpg',
      targetFolderId: 'folder-1',
      expectedSize: 1024,
      receivedBytes: 0,
      status: 'error',
      error: 'upload.errors.missingFile'
    })

    const wrapper = mount(UploadPanel, { global: { plugins: [i18n] } })
    const text = wrapper.text()

    expect(text).toContain(i18n.global.t('upload.errors.missingFile'))
    expect(text).not.toContain(i18n.global.t('upload.status.error'))
  })
})

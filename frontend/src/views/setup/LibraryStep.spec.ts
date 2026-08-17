import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import LibraryStep from './LibraryStep.vue'

vi.mock('@/api/libraries', () => ({
  previewLibraryPath: vi.fn(),
  createLibrary: vi.fn()
}))

const { previewLibraryPath, createLibrary } = await import('@/api/libraries')

afterEach(() => vi.resetAllMocks())

function mountStep() {
  return mount(LibraryStep, {
    global: { plugins: [i18n] }
  })
}

describe('LibraryStep', () => {
  it('cannot continue without a library', async () => {
    const wrapper = mountStep()
    const continueBtn = wrapper.findAll('button').find((b) => b.text().includes('Add library'))
    expect(continueBtn).toBeDefined()
    expect(continueBtn!.attributes('disabled')).toBeDefined()

    await continueBtn!.trigger('click')
    await flushPromises()
    expect(createLibrary).not.toHaveBeenCalled()
    expect(wrapper.emitted('created')).toBeUndefined()
  })

  it('preview shows real counts', async () => {
    vi.mocked(previewLibraryPath).mockResolvedValue({
      total: 42,
      extensions: { '.jpg': 40, '.arw': 2 }
    })

    const wrapper = mountStep()
    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(previewLibraryPath).toHaveBeenCalledWith('/photos')
    expect(wrapper.text()).toContain('42 files found')
    expect(wrapper.text()).toContain('.jpg: 40')
    expect(wrapper.text()).toContain('.arw: 2')
  })
})

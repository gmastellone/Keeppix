import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'

import { useUploadStore } from '@/stores/upload'

import { useUploadPicker } from './useUploadPicker'

function file(name: string): File {
  return new File([new Blob(['x'])], name)
}

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('useUploadPicker', () => {
  it('open() clicks the input ref passed in by the caller', () => {
    const input = document.createElement('input')
    const spy = vi.spyOn(input, 'click')
    const inputEl = ref<HTMLInputElement | null>(input)
    const { open } = useUploadPicker(inputEl)

    open()
    expect(spy).toHaveBeenCalledTimes(1)
  })

  it('onChange sends the picked files through the classifier and resets the input', async () => {
    const upload = useUploadStore()
    const spy = vi.spyOn(upload, 'addFilesFromPicker').mockImplementation(async () => {})
    const { onChange } = useUploadPicker(ref(null))

    const input = document.createElement('input')
    input.type = 'file'
    const picked = [file('a.jpg')]
    Object.defineProperty(input, 'files', { value: picked, configurable: true })

    await onChange({ target: input } as unknown as Event)

    expect(spy).toHaveBeenCalledWith(picked)
    expect(input.value).toBe('')
  })

  it('onChange with no files does nothing', async () => {
    const upload = useUploadStore()
    const spy = vi.spyOn(upload, 'addFilesFromPicker').mockImplementation(async () => {})
    const { onChange } = useUploadPicker(ref(null))

    const input = document.createElement('input')
    input.type = 'file'
    await onChange({ target: input } as unknown as Event)

    expect(spy).not.toHaveBeenCalled()
  })
})

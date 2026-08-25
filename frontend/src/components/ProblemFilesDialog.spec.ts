import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

const fetchChildrenMock = vi.fn()

vi.mock('@/api/folders', () => ({
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

const ProblemFilesDialog = (await import('./ProblemFilesDialog.vue')).default

// Stesso motivo di `AlbumPickerDialog.spec.ts`: il `DialogPortal` di
// reka-ui teletrasporta sempre nel vero `document.body`.
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function asset(id: string, filename: string) {
  return {
    id,
    folder_id: 'f',
    filename,
    content_hash: null,
    size_bytes: 1,
    kind: 'image' as const,
    status: 'indexed' as const,
    taken_at_utc: '2026-03-14T10:00:00Z',
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

let wrapper: VueWrapper | undefined

beforeEach(() => {
  setActivePinia(createPinia())
  i18n.global.locale.value = 'it'
  fetchChildrenMock.mockResolvedValue({ folders: [], assets: [] })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountDialog(folderId?: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/problems', component: { template: '<div />' } }
    ]
  })
  await router.push('/problems')
  await router.isReady()

  const Host = defineComponent({
    components: { TheDialog: ProblemFilesDialog },
    setup() {
      const open = ref(true)
      return { open, folderId }
    },
    template: `<TheDialog v-model:open="open" title="3 file con sidecar XMP non scrivibile" description="Chioggia e Venezia — permessi mancanti." :folder-id="folderId" />`
  })
  wrapper = mount(Host, { global: { plugins: [router, i18n] }, attachTo: document.body })
  await tick()
  return { wrapper, router }
}

describe('ProblemFilesDialog — §48', () => {
  it('shows the problem title/description and up to 3 files from the folder — not the files actually at fault', async () => {
    fetchChildrenMock.mockResolvedValue({
      folders: [],
      assets: [asset('a', 'IMG_1.jpg'), asset('b', 'IMG_2.jpg'), asset('c', 'IMG_3.jpg'), asset('d', 'IMG_4.jpg')]
    })
    await mountDialog('folder-1')

    expect(fetchChildrenMock).toHaveBeenCalledWith('folder-1')
    expect(document.body.textContent).toContain('3 file con sidecar XMP non scrivibile')
    expect(document.body.textContent).toContain('Chioggia e Venezia — permessi mancanti.')
    expect(document.body.textContent).toContain('IMG_1.jpg')
    expect(document.body.textContent).toContain('IMG_3.jpg')
    expect(document.body.textContent).not.toContain('IMG_4.jpg')
  })

  it('clicking a row navigates to Timeline with ?photo= and closes the dialog', async () => {
    fetchChildrenMock.mockResolvedValue({ folders: [], assets: [asset('a', 'IMG_1.jpg')] })
    const { router } = await mountDialog('folder-1')

    const row = document.body.querySelector('button[aria-label^="Apri"]') as HTMLButtonElement
    row.click()
    await tick()

    expect(router.currentRoute.value.path).toBe('/')
    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('"Chiudi" closes the dialog without navigating', async () => {
    const { router } = await mountDialog('folder-1')

    const closeBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Chiudi')
    closeBtn?.click()
    await tick()

    expect(router.currentRoute.value.path).toBe('/problems')
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('with no folder_id, shows no rows and does not call fetchChildren', async () => {
    await mountDialog(undefined)

    expect(fetchChildrenMock).not.toHaveBeenCalled()
    expect(document.body.querySelector('button[aria-label^="Apri"]')).toBeNull()
  })
})

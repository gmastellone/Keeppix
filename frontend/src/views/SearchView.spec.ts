import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import type { TimelineAsset } from '@/api/timeline'
import type { Tag } from '@/api/tags'
import type { FolderView } from '@/api/folders'
import type { SavedSearch, Suggestion } from '@/api/search'
import { runSearch } from '@/api/library'
import { createSavedSearch, fetchSavedSearches, fetchSuggestions } from '@/api/search'
import { fetchTags } from '@/api/tags'
import { fetchAllFolders, fetchTree } from '@/api/folders'
import PhotoTile from '@/components/ui/PhotoTile.vue'

import SearchView from './SearchView.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/search', () => ({
  fetchSavedSearches: vi.fn(async () => []),
  fetchSuggestions: vi.fn(async () => ({ suggestions: [] })),
  createSavedSearch: vi.fn(async (name: string, query_text: string) => ({ id: 's1', name, query_text }))
}))

vi.mock('@/api/tags', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/tags')>()
  return { ...actual, fetchTags: vi.fn(async () => []) }
})

vi.mock('@/api/folders', () => ({
  fetchAllFolders: vi.fn(async () => []),
  fetchTree: vi.fn(async () => [])
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { apiFetch } = await import('@/api/client')

// `FlatAssetGrid`/`SelectionBar` (Task 9 3/N, §25) montano `useDensity`/
// il layout giustificato, che leggono `clientWidth`/`clientHeight` e
// `window.matchMedia` — nessuno dei due esiste in jsdom di base. Stesso
// correttivo di `FavoritesView.spec.ts`.
function stubLayout(width: number, height: number) {
  const widthDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth')
  const heightDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: width })
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: height })
  return () => {
    if (widthDesc) Object.defineProperty(HTMLElement.prototype, 'clientWidth', widthDesc)
    if (heightDesc) Object.defineProperty(HTMLElement.prototype, 'clientHeight', heightDesc)
  }
}

function stubMatchMedia() {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      matches: false,
      media: '',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  )
}

let unstubLayout: (() => void) | undefined

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  unstubLayout?.()
  document.body.innerHTML = ''
})

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: 'ab'.repeat(32),
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
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

function tag(id: string, name: string, color: string | null = '#ffaa00'): Tag {
  return { id, name, kind: 'tag', parent_id: null, color, assignment_count: 1 }
}

function folder(id: string, name: string): FolderView {
  return { id, library_id: 'lib', parent_id: null, name, depth: 0 }
}

function suggestion(kind: Suggestion['kind'], value: string, label = value): Suggestion {
  return { kind, value, label, color: null }
}

/**
 * `apiFetchImpl` di proposito parametrizzabile: `AssetViewer.vue` apre il
 * pannello informazioni già aperto (§19.8) e carica il proprio giro di
 * dati (`loadPanelData`) subito al montaggio — un default a `[]` copre
 * gli endpoint che vogliono un array (tag/album/volti/stack); il test su
 * "?photo=" passa la propria implementazione perché deve rispondere
 * anche a `GET /assets/{id}`.
 */
async function mountSearch(
  initialPath = '/search',
  opts: {
    apiFetchImpl?: (path: string) => Promise<unknown>
    tags?: Tag[]
    folders?: FolderView[]
    suggestions?: Suggestion[]
    searchAssets?: TimelineAsset[]
    runSearchImpl?: (ast: { op: string }, cursor?: string) => Promise<{ assets: TimelineAsset[] }>
    roots?: FolderView[]
    savedSearches?: SavedSearch[]
    attachToBody?: boolean
  } = {}
) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/search', component: SearchView },
      { path: '/folders', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  if (opts.runSearchImpl) {
    vi.mocked(runSearch).mockImplementation(opts.runSearchImpl)
  } else {
    vi.mocked(runSearch).mockResolvedValue({ assets: opts.searchAssets ?? [] })
  }
  vi.mocked(fetchTree).mockResolvedValue(opts.roots ?? [])
  vi.mocked(fetchSavedSearches).mockResolvedValue(opts.savedSearches ?? [])
  vi.mocked(fetchSuggestions).mockResolvedValue({ suggestions: opts.suggestions ?? [] })
  vi.mocked(fetchTags).mockResolvedValue(opts.tags ?? [])
  vi.mocked(fetchAllFolders).mockResolvedValue(opts.folders ?? [])
  vi.mocked(apiFetch).mockImplementation(opts.apiFetchImpl ?? (async () => []))

  await router.push(initialPath)
  await router.isReady()
  const wrapper = mount(SearchView, {
    global: { plugins: [router, i18n] },
    attachTo: opts.attachToBody ? document.body : undefined
  })
  await flushPromises()
  return { router, wrapper }
}

describe('SearchView lightbox in the URL', () => {
  it('clicking a result pushes ?photo= (alongside the existing q) and opens the viewer', async () => {
    const { wrapper, router } = await mountSearch('/search?q=urbino', { searchAssets: [photo('a')] })

    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(router.currentRoute.value.query.q).toBe('urbino')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('reloading on a ?photo= URL restores the viewer by loading the asset directly', async () => {
    const { wrapper } = await mountSearch('/search?photo=a', {
      apiFetchImpl: async (path) => (path === '/api/v1/assets/a' ? photo('a') : [])
    })

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('closing the viewer removes ?photo= but keeps q', async () => {
    const { wrapper, router } = await mountSearch('/search?q=urbino', { searchAssets: [photo('a')] })
    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.photo).toBe('a')

    wrapper.findComponent({ name: 'AssetViewer' }).vm.$emit('close')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBeUndefined()
    expect(router.currentRoute.value.query.q).toBe('urbino')
  })
})

describe('SearchView — §23, il composer e i suggerimenti', () => {
  it('typing free text runs the search on every keystroke, no submit button involved', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch()

    expect(wrapper.find('button[type="submit"]').exists()).toBe(false)

    await wrapper.find('#search-query-input').setValue('tramonto')
    await flushPromises()

    expect(runSearch).toHaveBeenLastCalledWith({ op: 'text', value: 'tramonto' }, undefined)
  })

  it('focusing an empty field shows only the Tag and Cartella groups (§23.2, "incoraggia la scoperta")', async () => {
    const { wrapper } = await mountSearch('/search', {
      tags: [tag('t1', 'Paesaggi'), tag('t2', 'Tramonti')],
      folders: [folder('f1', 'Urbino')]
    })

    await wrapper.find('#search-query-input').trigger('focus')
    await flushPromises()

    const labels = wrapper.findAll('.search-suggest-group-label').map((el) => el.text())
    expect(labels).toEqual(['Tags', 'Folder'])
    expect(wrapper.text()).toContain('Paesaggi')
    expect(wrapper.text()).toContain('Urbino')
  })

  it('clicking a tag suggestion adds a pill, clears the text field, and searches by tag id', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')] })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()

    const row = wrapper.findAll('.search-suggest-row').find((el) => el.text().includes('Tramonti'))
    expect(row).toBeTruthy()
    await row?.trigger('click')
    await flushPromises()

    expect((wrapper.find('#search-query-input').element as HTMLInputElement).value).toBe('')
    expect(wrapper.find('.search-pill').text()).toContain('Tag: Tramonti')
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'tag', id: 't1' }, undefined)
  })

  it('a camera suggestion from the real backend becomes a pill labelled with the bare model name', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { suggestions: [suggestion('camera', 'Sony A7 IV')] })

    await wrapper.find('#search-query-input').setValue('sony')
    await flushPromises()

    const row = wrapper.findAll('.search-suggest-row').find((el) => el.text().includes('Sony A7 IV'))
    await row?.trigger('click')
    await flushPromises()

    expect(wrapper.find('.search-pill').text()).toContain('Sony A7 IV')
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'camera', value: 'Sony A7 IV' }, undefined)
  })

  it('the GPS pseudo-suggestion has no backend source and adds a has_gps pill', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch()

    await wrapper.find('#search-query-input').setValue('gp')
    await flushPromises()

    const row = wrapper.findAll('.search-suggest-row').find((el) => el.text().includes('Has GPS coordinates'))
    expect(row).toBeTruthy()
    await row?.trigger('click')
    await flushPromises()

    expect(wrapper.find('.search-pill').text()).toContain('With GPS position')
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'has_gps' }, undefined)
  })

  it('removing a pill via its × recomputes the search without it', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')] })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()
    await wrapper.findAll('.search-suggest-row')[0].trigger('click')
    await flushPromises()
    expect(wrapper.find('.search-pill').exists()).toBe(true)
    vi.mocked(runSearch).mockClear()

    await wrapper.find('.search-pill-x').trigger('click')
    await flushPromises()

    expect(wrapper.find('.search-pill').exists()).toBe(false)
    // Senza più alcuna pillola/testo, `hasSearch` torna falso: non "nessuna
    // ricerca" ma lo stato di scoperta (§25.2), che ricalcola la griglia
    // "Aggiunti di recente" con l'AST "tutto" — una sola pagina, non il
    // giro esaustivo di una vera ricerca.
    expect(runSearch).toHaveBeenCalledTimes(1)
    expect(runSearch).toHaveBeenCalledWith({ op: 'and', args: [] })
  })

  it('clear-all (#searchClearAll) empties pills and text together', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')] })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()
    await wrapper.findAll('.search-suggest-row')[0].trigger('click')
    await wrapper.find('#search-query-input').setValue('altro')
    await flushPromises()

    await wrapper.find('#searchClearAll').trigger('click')
    await flushPromises()

    expect(wrapper.find('.search-pill').exists()).toBe(false)
    expect((wrapper.find('#search-query-input').element as HTMLInputElement).value).toBe('')
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
  })

  it('a duplicate pill (same type and value already added) is not added twice', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')] })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()
    await wrapper.findAll('.search-suggest-row')[0].trigger('click')
    await flushPromises()

    // Con la pillola già presente, lo stesso tag non deve più comparire
    // tra i suggerimenti (`has(type,value)`, §23.2) — riscrivere lo
    // stesso testo non produce un secondo gruppo "Tag".
    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()

    expect(wrapper.findAll('.search-pill')).toHaveLength(1)
    expect(wrapper.findAll('.search-suggest-row').some((el) => el.text().includes('Tramonti'))).toBe(false)
  })

  it('the free-text row is purely informative: it has no click handler and does not add a pill', async () => {
    const { wrapper } = await mountSearch()

    await wrapper.find('#search-query-input').setValue('tramonto con casa')
    await flushPromises()

    const freeTextRow = wrapper.find('.search-suggest-free')
    expect(freeTextRow.text()).toContain('tramonto con casa')
    await freeTextRow.trigger('click')
    await flushPromises()

    expect(wrapper.find('.search-pill').exists()).toBe(false)
  })

  it('Esc closes the suggestion panel without clearing pills or text', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch()

    await wrapper.find('#search-query-input').setValue('tramonto')
    await flushPromises()
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true)

    await wrapper.find('#search-query-input').trigger('keydown', { key: 'Escape' })
    await flushPromises()

    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
    expect((wrapper.find('#search-query-input').element as HTMLInputElement).value).toBe('tramonto')
  })

  it('clicking outside the composer closes the suggestion panel (§23.4, listener a livello di document)', async () => {
    const { wrapper } = await mountSearch()

    await wrapper.find('#search-query-input').trigger('focus')
    await flushPromises()
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true)

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
    await flushPromises()

    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
  })

  it('ArrowDown from the input moves focus onto the first suggestion row (accessibility gap the doc calls out, §23.5)', async () => {
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')], attachToBody: true })

    await wrapper.find('#search-query-input').trigger('focus')
    await flushPromises()
    await wrapper.find('#search-query-input').trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()

    expect(document.activeElement?.classList.contains('search-suggest-row')).toBe(true)
    expect(document.activeElement?.textContent).toContain('Tramonti')

    wrapper.unmount()
  })
})

describe('SearchView — §23.3, i chip del tipo file', () => {
  function chip(wrapper: ReturnType<typeof mount>, label: string) {
    return wrapper.findAll('button').find((el) => el.text() === label)
  }

  it('"Tutti i tipi" is the default and the four chips are mutually exclusive', async () => {
    const { wrapper } = await mountSearch()

    const all = chip(wrapper, 'All types')
    const raw = chip(wrapper, 'RAW')
    expect(all?.classes()).toContain('text-accent')
    expect(raw?.classes()).not.toContain('text-accent')

    await raw?.trigger('click')
    await flushPromises()

    expect(chip(wrapper, 'RAW')?.classes()).toContain('text-accent')
    expect(chip(wrapper, 'All types')?.classes()).not.toContain('text-accent')
  })

  it('clicking RAW searches by type raw_image; JPEG searches by type image (not just "not raw" — video stays excluded)', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch()

    // Il chip da solo non conta come "ricerca" (§25.2): resta lo stato di
    // scoperta, quindi la chiamata passa per `loadRecent()` — una sola
    // pagina, senza cursore — non per il giro esaustivo di una ricerca vera.
    await chip(wrapper, 'RAW')?.trigger('click')
    await flushPromises()
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'type', value: 'raw_image' })

    await chip(wrapper, 'JPEG')?.trigger('click')
    await flushPromises()
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'type', value: 'image' })
  })

  it('"Preferiti" searches by favorite, combined in AND with a pill when both are active', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch('/search', { tags: [tag('t1', 'Tramonti')] })

    await chip(wrapper, 'Favorites')?.trigger('click')
    await flushPromises()
    expect(runSearch).toHaveBeenLastCalledWith({ op: 'favorite' })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()
    await wrapper.findAll('.search-suggest-row')[0].trigger('click')
    await flushPromises()

    expect(runSearch).toHaveBeenLastCalledWith(
      { op: 'and', args: [{ op: 'favorite' }, { op: 'tag', id: 't1' }] },
      undefined
    )
  })

  it('"Persona" is disabled: no click handler, native title tooltip, cannot become active', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountSearch()
    vi.mocked(runSearch).mockClear()

    const person = wrapper.findAll('span').find((el) => el.text() === 'Person')
    expect(person?.attributes('title')).toBe('Requires face recognition — see Group B')

    await person?.trigger('click')
    await flushPromises()
    expect(runSearch).not.toHaveBeenCalled()
  })

  it('clear-all does not reset the type chip (§23.3, "non azzera il chip del tipo file")', async () => {
    const { wrapper } = await mountSearch()

    await chip(wrapper, 'RAW')?.trigger('click')
    await flushPromises()
    await wrapper.find('#searchClearAll').trigger('click')
    await flushPromises()

    expect(chip(wrapper, 'RAW')?.classes()).toContain('text-accent')
  })
})

describe('SearchView — §25, area risultati e scoperta', () => {
  it('the discovery state (no search) shows saved searches, folder cards with a real recursive count, and "Recently added"', async () => {
    const { wrapper } = await mountSearch('/search', {
      roots: [folder('f1', 'Urbino')],
      savedSearches: [{ id: 's1', name: 'RAW · Urbino', query_text: 'type:raw_image folder:f1' }],
      runSearchImpl: async (ast) => (ast.op === 'folder' ? { assets: [photo('a'), photo('b')] } : { assets: [photo('c')] })
    })

    expect(wrapper.text()).toContain('Recently added')
    expect(wrapper.text()).toContain('Saved searches')
    expect(wrapper.text()).toContain('RAW · Urbino')
    expect(wrapper.text()).toContain('Urbino')
    expect(wrapper.text()).toContain('2 photos')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(1)
  })

  it('the results state shows "Results", the recap with each part bolded, and the result count', async () => {
    const { wrapper } = await mountSearch('/search', { searchAssets: [photo('a')] })

    await wrapper.find('#search-query-input').setValue('tramonto')
    await flushPromises()

    expect(wrapper.text()).toContain('Results')
    expect(wrapper.findAll('b').map((el) => el.text())).toContain('free description “tramonto”')
    expect(wrapper.text()).toContain('1 result')
  })

  it('an empty result set shows the "No results" empty state, not the grid', async () => {
    const { wrapper } = await mountSearch('/search', { searchAssets: [] })

    await wrapper.find('#search-query-input').setValue('xyz123nomatch')
    await flushPromises()

    expect(wrapper.text()).toContain('No results')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(0)
  })

  it('"Save this search" is disabled for a tag pill, and serializes a camera pill + free text otherwise', async () => {
    const { wrapper } = await mountSearch('/search', {
      tags: [tag('t1', 'Tramonti')],
      suggestions: [suggestion('camera', 'Sony A7 IV')]
    })

    await wrapper.find('#search-query-input').setValue('tram')
    await flushPromises()
    await wrapper.findAll('.search-suggest-row')[0].trigger('click')
    await flushPromises()
    let saveBtn = wrapper.findAll('button').find((el) => el.text().includes('Save this search'))
    expect(saveBtn?.attributes('disabled')).toBeDefined()

    await wrapper.find('#searchClearAll').trigger('click')
    await flushPromises()

    await wrapper.find('#search-query-input').setValue('sony')
    await flushPromises()
    const cameraRow = wrapper.findAll('.search-suggest-row').find((el) => el.text().includes('Sony A7 IV'))
    await cameraRow?.trigger('click')
    await flushPromises()
    await wrapper.find('#search-query-input').setValue('tramonto con casa')
    await flushPromises()

    saveBtn = wrapper.findAll('button').find((el) => el.text().includes('Save this search'))
    expect(saveBtn?.attributes('disabled')).toBeUndefined()
    await saveBtn?.trigger('click')
    await flushPromises()

    expect(createSavedSearch).toHaveBeenCalledWith(
      'Sony A7 IV + tramonto con casa',
      'camera:"Sony A7 IV" "tramonto con casa"'
    )
    expect(wrapper.text()).toContain('Saved ✓')
  })

  it('clicking a folder card navigates to /folders — no folder-scoped timeline route exists in the real app', async () => {
    const { wrapper, router } = await mountSearch('/search', {
      roots: [folder('f1', 'Urbino')],
      runSearchImpl: async (ast) => (ast.op === 'folder' ? { assets: [photo('a')] } : { assets: [] })
    })

    const card = wrapper.findAll('button').find((el) => el.text().includes('Urbino'))
    await card?.trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/folders')
  })
})

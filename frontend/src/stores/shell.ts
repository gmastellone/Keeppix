// Fase 11 Task 6: i dati della shell (sidebar desktop, header/"Altro"
// mobile) — cartelle, spazio libero, badge di navigazione — dietro
// un'unica chiamata a `GET /api/v1/bootstrap`. Uno store a sé, non parte
// di `stores/session.ts`: quello risponde "chi sono e sono autenticato",
// questo risponde "cosa mostra il telaio" — due domande diverse anche se
// il backend chiama entrambe le richieste "bootstrap".
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { fetchBootstrap, type BadgeCountsView, type LibraryStorageView } from '@/api/bootstrap'
import type { FolderView } from '@/api/folders'

export const useShellStore = defineStore('shell', () => {
  const folders = ref<FolderView[]>([])
  const storage = ref<Record<string, LibraryStorageView>>({})
  const badges = ref<BadgeCountsView>({ culling: 0, revision: 0 })
  const loaded = ref(false)

  async function load(): Promise<void> {
    const data = await fetchBootstrap()
    folders.value = data.folders
    storage.value = data.storage
    badges.value = data.badges
    loaded.value = true
  }

  return { folders, storage, badges, loaded, load }
})

// Shell data (desktop sidebar, mobile header/"More") — folders, free
// space, navigation badges — behind a single call to
// `GET /api/v1/bootstrap`. A store of its own, not part of
// `stores/session.ts`: that one answers "who am I and am I
// authenticated", this one answers "what does the shell show" — two
// different questions even though the backend calls both requests
// "bootstrap".
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

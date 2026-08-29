<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import {
  fetchChildren,
  fetchTree,
  moveFolder,
  type FolderChildren,
  type FolderView
} from '@/api/folders'
import { isUnauthenticated } from '@/api/client'
import FolderBranch from '@/components/FolderBranch.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const roots = ref<FolderView[]>([])
const kids = ref<Record<string, FolderChildren>>({})
const open = ref<Set<string>>(new Set())
const loadError = ref(false)

onMounted(() => {
  void loadRoots()
})

async function loadRoots() {
  loadError.value = false
  try {
    roots.value = await fetchTree()
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function toggle(id: string) {
  if (open.value.has(id)) {
    const next = new Set(open.value)
    next.delete(id)
    open.value = next
    return
  }
  if (!kids.value[id]) {
    kids.value = { ...kids.value, [id]: await fetchChildren(id) }
  }
  open.value = new Set(open.value).add(id)
}

function destinationFor(folder: FolderView): string | undefined {
  const siblings =
    (folder.parent_id ? kids.value[folder.parent_id]?.folders : roots.value) ?? []
  return siblings.find((s) => s.id !== folder.id)?.id
}

async function relocate(folder: FolderView) {
  const dest = destinationFor(folder)
  if (!dest) return
  await moveFolder(folder.id, dest)
  if (folder.parent_id) {
    kids.value = { ...kids.value, [folder.parent_id]: await fetchChildren(folder.parent_id) }
  }
}
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <!-- No back link or title here: AppSidebar (the "Folders" entry) and
         AppTopbar (the "Folders" breadcrumb) already cover that. -->
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <p
      v-else-if="roots.length === 0"
      class="mt-6 text-content-muted"
    >
      {{ t('folders.empty') }}
    </p>
    <ul
      v-else
      class="mt-6 space-y-1"
    >
      <FolderBranch
        v-for="folder in roots"
        :key="folder.id"
        :folder="folder"
        :kids="kids"
        :open="open"
        @toggle="toggle"
        @move="relocate"
      />
    </ul>
  </main>
</template>

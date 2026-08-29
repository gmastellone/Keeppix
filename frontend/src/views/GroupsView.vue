<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { isUnauthenticated } from '@/api/client'
import { createGroup, deleteGroup, fetchGroups, type Group } from '@/api/groups'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const groups = ref<Group[]>([])
const loadError = ref(false)
const newName = ref('')

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    groups.value = await fetchGroups()
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function add() {
  if (!newName.value.trim()) return
  await createGroup(newName.value.trim())
  newName.value = ''
  await load()
}

async function remove(id: string) {
  await deleteGroup(id)
  await load()
}
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <!-- No back link or title here: AppSidebar (the "Groups" entry, under
         Administration) and AppTopbar (the "Groups" breadcrumb) already
         cover that. -->
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <template v-else>
      <form
        class="mt-4 flex gap-2"
        @submit.prevent="add"
      >
        <input
          v-model="newName"
          class="flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
          :placeholder="t('groups.namePlaceholder')"
        >
        <button
          class="rounded-lg bg-accent px-4 py-2 text-sm text-white"
          type="submit"
        >
          {{ t('groups.create') }}
        </button>
      </form>
      <p
        v-if="groups.length === 0"
        class="mt-6 text-content-muted"
      >
        {{ t('groups.empty') }}
      </p>
      <ul
        v-else
        class="mt-4 space-y-2"
      >
        <li
          v-for="group in groups"
          :key="group.id"
          class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
        >
          <div>
            <span class="font-medium">{{ group.name }}</span>
            <span class="ml-2 text-sm text-content-muted">
              {{ t('groups.memberCount', { count: group.member_count }) }}
            </span>
          </div>
          <button
            class="text-sm text-danger underline"
            @click="remove(group.id)"
          >
            {{ t('groups.delete') }}
          </button>
        </li>
      </ul>
    </template>
  </main>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { createGroup, deleteGroup, fetchGroups, type Group } from '@/api/groups'

const { t } = useI18n()

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
  } catch {
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
    <p class="mb-4 text-sm">
      <RouterLink
        class="underline"
        to="/"
      >
        {{ t('folders.back') }}
      </RouterLink>
    </p>
    <h1 class="text-2xl font-semibold">
      {{ t('groups.title') }}
    </h1>
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

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchUsers, type UserSummary } from '@/api/users'
import { fetchAuditLog } from '@/api/audit'

const { t } = useI18n()

const users = ref<UserSummary[]>([])
const loadError = ref(false)

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    users.value = await fetchUsers()
    await fetchAuditLog(1).catch(() => undefined)
  } catch {
    loadError.value = true
  }
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
      {{ t('users.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <p
      v-else-if="users.length === 0"
      class="mt-6 text-content-muted"
    >
      {{ t('users.empty') }}
    </p>
    <ul
      v-else
      class="mt-4 space-y-2"
    >
      <li
        v-for="user in users"
        :key="user.id"
        class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
      >
        <div>
          <span class="font-medium">{{ user.display_name }}</span>
          <span class="ml-2 text-sm text-content-muted">{{ user.username }}</span>
          <span
            v-if="user.role === 'admin'"
            class="ml-2 rounded bg-accent px-1.5 py-0.5 text-xs text-white"
          >
            {{ t('users.admin') }}
          </span>
        </div>
      </li>
    </ul>
  </main>
</template>

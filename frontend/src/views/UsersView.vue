<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { fetchAuditLog } from '@/api/audit'
import { isUnauthenticated } from '@/api/client'
import {
  changePassword,
  createUser,
  deleteHome,
  disableUser,
  enableUser,
  fetchUsers,
  setHome,
  updateUser,
  type HomeLocation,
  type UserSummary
} from '@/api/users'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const users = ref<UserSummary[]>([])
const loadError = ref(false)

const newUsername = ref('')
const newDisplayName = ref('')
const newPassword = ref('')
const newRole = ref<'user' | 'admin'>('user')
const creating = ref(false)

const currentPassword = ref('')
const nextPassword = ref('')
const savingPassword = ref(false)

const homeLat = ref('')
const homeLon = ref('')
const homeRadius = ref('200')
const homeLoaded = ref<HomeLocation | null>(null)
const savingHome = ref(false)

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    users.value = await fetchUsers()
    await fetchAuditLog(1).catch(() => undefined)
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function create() {
  if (!newUsername.value.trim() || !newDisplayName.value.trim() || !newPassword.value || creating.value) {
    return
  }
  creating.value = true
  try {
    await createUser({
      username: newUsername.value.trim(),
      display_name: newDisplayName.value.trim(),
      password: newPassword.value,
      role: newRole.value
    })
    newUsername.value = ''
    newDisplayName.value = ''
    newPassword.value = ''
    newRole.value = 'user'
    await load()
  } finally {
    creating.value = false
  }
}

async function setRole(id: string, event: Event) {
  const role = (event.target as HTMLSelectElement).value
  await updateUser(id, { role })
  await load()
}

async function disable(id: string) {
  await disableUser(id)
  await load()
}

async function enable(id: string) {
  await enableUser(id)
  await load()
}

async function savePassword() {
  if (!currentPassword.value || !nextPassword.value || savingPassword.value) return
  savingPassword.value = true
  try {
    await changePassword(currentPassword.value, nextPassword.value)
    currentPassword.value = ''
    nextPassword.value = ''
  } finally {
    savingPassword.value = false
  }
}

async function saveHome() {
  const lat = parseFloat(homeLat.value)
  const lon = parseFloat(homeLon.value)
  const radius = parseInt(homeRadius.value, 10) || 200
  if (Number.isNaN(lat) || Number.isNaN(lon) || savingHome.value) return
  savingHome.value = true
  try {
    homeLoaded.value = await setHome(lat, lon, radius)
  } finally {
    savingHome.value = false
  }
}

async function removeHome() {
  savingHome.value = true
  try {
    await deleteHome()
    homeLoaded.value = null
    homeLat.value = ''
    homeLon.value = ''
    homeRadius.value = '200'
  } finally {
    savingHome.value = false
  }
}

function isSelf(id: string): boolean {
  return session.user?.id === id
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
    <template v-else>
      <form
        class="mt-6 grid gap-3 sm:grid-cols-2"
        @submit.prevent="create"
      >
        <h2 class="text-lg font-semibold sm:col-span-2">
          {{ t('users.createTitle') }}
        </h2>
        <label class="block text-sm">
          {{ t('users.username') }}
          <input
            v-model="newUsername"
            data-testid="users-username"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            autocomplete="off"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.displayName') }}
          <input
            v-model="newDisplayName"
            data-testid="users-display-name"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.password') }}
          <input
            v-model="newPassword"
            data-testid="users-password"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="password"
            autocomplete="new-password"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.role') }}
          <select
            v-model="newRole"
            data-testid="users-role"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
          >
            <option value="user">
              {{ t('users.roleUser') }}
            </option>
            <option value="admin">
              {{ t('users.roleAdmin') }}
            </option>
          </select>
        </label>
        <button
          class="rounded-lg bg-accent px-4 py-2 text-sm text-white sm:col-span-2"
          data-testid="users-create"
          type="button"
          @click="create"
        >
          {{ t('users.create') }}
        </button>
      </form>

      <p
        v-if="users.length === 0"
        class="mt-6 text-content-muted"
      >
        {{ t('users.empty') }}
      </p>
      <ul
        v-else
        data-testid="users-list"
        class="mt-6 space-y-2"
      >
        <li
          v-for="user in users"
          :key="user.id"
          class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border px-4 py-3"
        >
          <div>
            <span class="font-medium">{{ user.display_name }}</span>
            <span class="ml-2 text-sm text-content-muted">{{ user.username }}</span>
            <span
              v-if="user.disabled_at"
              class="ml-2 text-xs text-content-muted"
            >
              {{ t('users.disabled') }}
            </span>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <select
              :value="user.role"
              class="rounded-lg border border-border bg-surface-elevated px-2 py-1 text-sm"
              :data-testid="`users-role-${user.id}`"
              :disabled="isSelf(user.id)"
              @change="setRole(user.id, $event)"
            >
              <option value="user">
                {{ t('users.roleUser') }}
              </option>
              <option value="admin">
                {{ t('users.roleAdmin') }}
              </option>
            </select>
            <button
              v-if="!isSelf(user.id) && !user.disabled_at"
              class="text-sm text-danger underline"
              type="button"
              :data-testid="`users-disable-${user.id}`"
              @click="disable(user.id)"
            >
              {{ t('users.disable') }}
            </button>
            <button
              v-if="!isSelf(user.id) && user.disabled_at"
              class="text-sm underline"
              type="button"
              :data-testid="`users-enable-${user.id}`"
              @click="enable(user.id)"
            >
              {{ t('users.enable') }}
            </button>
          </div>
        </li>
      </ul>

      <form
        class="mt-10 grid gap-3 sm:grid-cols-2"
        @submit.prevent="savePassword"
      >
        <h2 class="text-lg font-semibold sm:col-span-2">
          {{ t('users.passwordTitle') }}
        </h2>
        <label class="block text-sm">
          {{ t('users.currentPassword') }}
          <input
            v-model="currentPassword"
            data-testid="users-current-password"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="password"
            autocomplete="current-password"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.newPassword') }}
          <input
            v-model="nextPassword"
            data-testid="users-new-password"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="password"
            autocomplete="new-password"
          >
        </label>
        <button
          class="rounded-lg bg-accent px-4 py-2 text-sm text-white sm:col-span-2"
          data-testid="users-password-save"
          type="button"
          @click="savePassword"
        >
          {{ t('users.savePassword') }}
        </button>
      </form>
      <form
        class="mt-10 grid gap-3 sm:grid-cols-3"
        @submit.prevent="saveHome"
      >
        <h2 class="text-lg font-semibold sm:col-span-3">
          {{ t('users.homeTitle') }}
        </h2>
        <label class="block text-sm">
          {{ t('users.homeLat') }}
          <input
            v-model="homeLat"
            data-testid="home-lat"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="number"
            step="any"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.homeLon') }}
          <input
            v-model="homeLon"
            data-testid="home-lon"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="number"
            step="any"
          >
        </label>
        <label class="block text-sm">
          {{ t('users.homeRadius') }}
          <input
            v-model="homeRadius"
            data-testid="home-radius"
            class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            type="number"
            min="1"
          >
        </label>
        <div class="flex gap-2 sm:col-span-3">
          <button
            class="rounded-lg bg-accent px-4 py-2 text-sm text-white"
            data-testid="home-save"
            type="button"
            :disabled="savingHome"
            @click="saveHome"
          >
            {{ t('users.homeSave') }}
          </button>
          <button
            v-if="homeLoaded"
            class="rounded-lg border border-danger px-4 py-2 text-sm text-danger"
            data-testid="home-delete"
            type="button"
            :disabled="savingHome"
            @click="removeHome"
          >
            {{ t('users.homeDelete') }}
          </button>
        </div>
      </form>
    </template>
  </main>
</template>

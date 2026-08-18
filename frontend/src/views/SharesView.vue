<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchTree, type FolderView } from '@/api/folders'
import { fetchGroups, type Group } from '@/api/groups'
import {
  explainPermission,
  fetchPermissions,
  grantPermission,
  revokePermission,
  type ExplainChainLink,
  type PermissionGrant
} from '@/api/permissions'
import { fetchShareLinks, revokeShareLink, type ShareLink } from '@/api/shares'
import { fetchUsers, type UserSummary } from '@/api/users'

const { t } = useI18n()

const links = ref<ShareLink[]>([])
const folders = ref<FolderView[]>([])
const users = ref<UserSummary[]>([])
const groups = ref<Group[]>([])
const grants = ref<PermissionGrant[]>([])
const loadError = ref(false)

const folderId = ref('')
const subjectType = ref<'user' | 'group'>('user')
const subjectId = ref('')
const role = ref<'viewer' | 'editor'>('viewer')
const inherit = ref(true)
const granting = ref(false)

const explainUserId = ref('')
const explainChain = ref<ExplainChainLink[] | null>(null)
const explainGranted = ref<boolean | null>(null)

const subjects = computed(() =>
  subjectType.value === 'user'
    ? users.value.map((u) => ({ id: u.id, label: u.display_name }))
    : groups.value.map((g) => ({ id: g.id, label: g.name }))
)

onMounted(() => {
  void load()
})

watch(folderId, () => {
  explainChain.value = null
  explainGranted.value = null
  void loadGrants()
})

watch(subjectType, () => {
  subjectId.value = ''
})

async function load() {
  loadError.value = false
  try {
    const [linkRows, tree, userRows, groupRows] = await Promise.all([
      fetchShareLinks(),
      fetchTree(),
      fetchUsers().catch(() => [] as UserSummary[]),
      fetchGroups().catch(() => [] as Group[])
    ])
    links.value = linkRows
    folders.value = tree
    users.value = userRows
    groups.value = groupRows
  } catch {
    loadError.value = true
  }
}

async function loadGrants() {
  if (!folderId.value) {
    grants.value = []
    return
  }
  grants.value = await fetchPermissions('folder', folderId.value)
}

async function grant() {
  if (!folderId.value || !subjectId.value || granting.value) return
  granting.value = true
  try {
    await grantPermission({
      subject_type: subjectType.value,
      subject_id: subjectId.value,
      object_type: 'folder',
      object_id: folderId.value,
      role: role.value,
      inherit: inherit.value
    })
    await loadGrants()
  } finally {
    granting.value = false
  }
}

async function removeGrant(id: string) {
  await revokePermission(id)
  await loadGrants()
}

async function explain() {
  if (!folderId.value || !explainUserId.value) return
  const result = await explainPermission('folder', folderId.value, explainUserId.value)
  explainGranted.value = result.granted
  explainChain.value = result.chain
}

async function removeLink(id: string) {
  await revokeShareLink(id)
  await load()
}

function subjectLabel(grant: PermissionGrant): string {
  if (grant.subject_type === 'group') {
    return groups.value.find((g) => g.id === grant.subject_id)?.name ?? grant.subject_id
  }
  return users.value.find((u) => u.id === grant.subject_id)?.display_name ?? grant.subject_id
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
      {{ t('shares.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <template v-else>
      <section class="mt-8">
        <h2 class="text-lg font-semibold">
          {{ t('shares.peopleTitle') }}
        </h2>
        <form
          class="mt-4 grid gap-3 sm:grid-cols-2"
          @submit.prevent="grant"
        >
          <label class="block text-sm">
            {{ t('shares.folder') }}
            <select
              v-model="folderId"
              data-testid="shares-folder"
              class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option value="">
                {{ t('shares.chooseFolder') }}
              </option>
              <option
                v-for="folder in folders"
                :key="folder.id"
                :value="folder.id"
              >
                {{ folder.name }}
              </option>
            </select>
          </label>
          <label class="block text-sm">
            {{ t('shares.subjectType') }}
            <select
              v-model="subjectType"
              data-testid="shares-subject-type"
              class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option value="user">
                {{ t('shares.person') }}
              </option>
              <option value="group">
                {{ t('shares.group') }}
              </option>
            </select>
          </label>
          <label class="block text-sm">
            {{ t('shares.subject') }}
            <select
              v-model="subjectId"
              data-testid="shares-subject"
              class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option value="">
                {{ t('shares.chooseSubject') }}
              </option>
              <option
                v-for="subject in subjects"
                :key="subject.id"
                :value="subject.id"
              >
                {{ subject.label }}
              </option>
            </select>
          </label>
          <label class="block text-sm">
            {{ t('shares.role') }}
            <select
              v-model="role"
              data-testid="shares-role"
              class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option value="viewer">
                {{ t('shares.viewer') }}
              </option>
              <option value="editor">
                {{ t('shares.editor') }}
              </option>
            </select>
          </label>
          <label class="flex items-center gap-2 text-sm sm:col-span-2">
            <input
              v-model="inherit"
              type="checkbox"
            >
            {{ t('shares.inherit') }}
          </label>
          <button
            class="rounded-lg bg-accent px-4 py-2 text-sm text-white sm:col-span-2"
            data-testid="shares-grant"
            type="button"
            @click="grant"
          >
            {{ t('shares.grant') }}
          </button>
        </form>

        <p
          v-if="folderId && grants.length === 0"
          class="mt-4 text-sm text-content-muted"
        >
          {{ t('shares.grantsEmpty') }}
        </p>
        <ul
          v-else-if="grants.length > 0"
          data-testid="shares-grants"
          class="mt-4 space-y-2"
        >
          <li
            v-for="grantRow in grants"
            :key="grantRow.id"
            class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
          >
            <p class="text-sm">
              {{ subjectLabel(grantRow) }} · {{ grantRow.role }}
            </p>
            <button
              class="ml-4 text-sm text-danger underline"
              type="button"
              @click="removeGrant(grantRow.id)"
            >
              {{ t('shares.revoke') }}
            </button>
          </li>
        </ul>

        <h3 class="mt-8 text-base font-semibold">
          {{ t('shares.explainTitle') }}
        </h3>
        <div class="mt-3 flex flex-wrap items-end gap-2">
          <label class="block min-w-48 flex-1 text-sm">
            {{ t('shares.person') }}
            <select
              v-model="explainUserId"
              data-testid="shares-explain-user"
              class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option value="">
                {{ t('shares.chooseSubject') }}
              </option>
              <option
                v-for="user in users"
                :key="user.id"
                :value="user.id"
              >
                {{ user.display_name }}
              </option>
            </select>
          </label>
          <button
            class="rounded-lg border border-border px-4 py-2 text-sm"
            data-testid="shares-explain"
            type="button"
            @click="explain"
          >
            {{ t('shares.explain') }}
          </button>
        </div>
        <p
          v-if="explainGranted === false"
          class="mt-3 text-sm text-content-muted"
        >
          {{ t('shares.explainNone') }}
        </p>
        <ul
          v-else-if="explainChain && explainChain.length > 0"
          data-testid="shares-explain-chain"
          class="mt-3 space-y-1 text-sm"
        >
          <li
            v-for="(link, index) in explainChain"
            :key="index"
          >
            {{
              link.inherited_in
                ? t('shares.explainLinkInherited', {
                    subject: link.subject_name,
                    role: link.role,
                    on: link.granted_on_name,
                    inherited: link.inherited_in
                  })
                : t('shares.explainLink', {
                    subject: link.subject_name,
                    role: link.role,
                    on: link.granted_on_name
                  })
            }}
          </li>
        </ul>
      </section>

      <section class="mt-10">
        <h2 class="text-lg font-semibold">
          {{ t('shares.linksTitle') }}
        </h2>
        <p
          v-if="links.length === 0"
          class="mt-4 text-content-muted"
        >
          {{ t('shares.empty') }}
        </p>
        <ul
          v-else
          class="mt-4 space-y-2"
        >
          <li
            v-for="link in links"
            :key="link.id"
            class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
          >
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm">
                {{ link.object_type }} · {{ link.object_id }}
              </p>
              <p class="text-xs text-content-muted">
                {{ t('shares.views', { count: link.view_count }) }}
              </p>
            </div>
            <button
              class="ml-4 text-sm text-danger underline"
              @click="removeLink(link.id)"
            >
              {{ t('shares.revoke') }}
            </button>
          </li>
        </ul>
      </section>
    </template>
  </main>
</template>

<script setup lang="ts">
// A full "Shares" page, with two tabs ("My shares"/"Shared with me") and
// three sections (People/Public links/Shared folders and albums).
//
// **"People" section — admin only**: listing "who I've shared with"
// requires resolving subject name/email (`GET /users`/`GET /groups`),
// both reserved to `AdminAuth`
// (`crates/keeppix-api/src/routes/users.rs`/`groups.rs`) — no alternative
// route exists for a regular user. Same choice already made for
// `ShareSelectionDialog.vue`: the section stays hidden for non-admins,
// the rest of the page (Public links, Shared folders and albums, Shared
// with me) still works for anyone.
//
// **"Copy" only exists for a link just created in this session**:
// `GET /share/links` (`LinkView`) never includes the `token` — only the
// creation response returns it, once
// (`crates/keeppix-api/src/routes/share.rs`). A link loaded from the
// list has no way to reconstruct the shareable URL: showing "Copy" for
// those links would promise an action that can't work. The
// `justCreatedTokens` map only covers links created by this page, in
// this load.
//
// **The "Shared folders and albums" cards are clickable**. No "photos
// scoped to a folder"/"album detail" view exists yet: they lead to
// `/folders` and `/albums` respectively, the closest real destinations.
//
// **"Create share link" at the bottom of the Public links section is not
// built here**: this page creates and manages existing links, it doesn't
// generate new ones from scratch with no target; the real way to create
// one is `ShareSelectionDialog.vue`, from a grid with a selection.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { fetchAlbumAssets, fetchAlbums, type Album } from '@/api/albums'
import { isUnauthenticated } from '@/api/client'
import { fetchAllFolders, fetchTree, type FolderView } from '@/api/folders'
import { fetchGroups, type Group } from '@/api/groups'
import { runSearch } from '@/api/library'
import {
  explainPermission,
  fetchPermissions,
  fetchSharedWithMe,
  grantPermission,
  revokePermission,
  type ExplainChainLink,
  type PermissionGrant,
  type SharedWithMe
} from '@/api/permissions'
import { fetchShareLinks, revokeShareLink, type ShareLink } from '@/api/shares'
import { fetchUsers, type UserSummary } from '@/api/users'
import { avatarColorFor } from '@/design/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import Avatar from '@/components/ui/Avatar.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

const isAdmin = computed(() => session.user?.role === 'admin')

const tab = ref<'mine' | 'shared'>(route.query.tab === 'shared' ? 'shared' : 'mine')
watch(tab, (value) => {
  void router.replace({ query: { ...route.query, tab: value } })
})

const loadError = ref(false)
const links = ref<ShareLink[]>([])
const allFolders = ref<FolderView[]>([])
const albums = ref<Album[]>([])
const sharedWithMe = ref<SharedWithMe[]>([])
const justCreatedTokens = ref<Record<string, string>>({})

interface PersonRow {
  grantId: string
  subjectType: string
  subjectId: string
  role: string
  inherited: boolean
  objectType: 'folder' | 'album'
  objectId: string
  objectName: string
}

const personRows = ref<PersonRow[]>([])
const users = ref<UserSummary[]>([])
const groups = ref<Group[]>([])

async function load() {
  loadError.value = false
  try {
    const [linkRows, folderRows, albumRows, sharedRows] = await Promise.all([
      fetchShareLinks(),
      fetchAllFolders(),
      fetchAlbums(),
      fetchSharedWithMe()
    ])
    links.value = linkRows
    allFolders.value = folderRows
    albums.value = albumRows
    sharedWithMe.value = sharedRows
    if (isAdmin.value) await loadPeople()
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function loadPeople() {
  const [userRows, groupRows] = await Promise.all([
    fetchUsers().catch(() => [] as UserSummary[]),
    fetchGroups().catch(() => [] as Group[])
  ])
  users.value = userRows
  groups.value = groupRows

  const folderGroups = await Promise.all(
    allFolders.value.map(async (folder) => ({
      objectType: 'folder' as const,
      objectId: folder.id,
      objectName: folder.name,
      grants: await fetchPermissions('folder', folder.id).catch(() => [] as PermissionGrant[])
    }))
  )
  const albumGroups = await Promise.all(
    albums.value.map(async (album) => ({
      objectType: 'album' as const,
      objectId: album.id,
      objectName: album.name,
      grants: await fetchPermissions('album', album.id).catch(() => [] as PermissionGrant[])
    }))
  )
  const rows: PersonRow[] = []
  for (const group of [...folderGroups, ...albumGroups]) {
    for (const grant of group.grants) {
      rows.push({
        grantId: grant.id,
        subjectType: grant.subject_type,
        subjectId: grant.subject_id,
        role: grant.role,
        inherited: grant.inherited,
        objectType: group.objectType,
        objectId: group.objectId,
        objectName: group.objectName
      })
    }
  }
  personRows.value = rows
}

function subjectName(row: PersonRow): string {
  if (row.subjectType === 'group') {
    return groups.value.find((g) => g.id === row.subjectId)?.name ?? row.subjectId
  }
  return users.value.find((u) => u.id === row.subjectId)?.display_name ?? row.subjectId
}

function subjectEmail(row: PersonRow): string | undefined {
  if (row.subjectType !== 'user') return undefined
  return users.value.find((u) => u.id === row.subjectId)?.username
}

function roleLabel(role: string): string {
  return role === 'editor' ? t('shares.editor') : t('shares.viewer')
}

async function revokeGrant(row: PersonRow) {
  await revokePermission(row.grantId)
  personRows.value = personRows.value.filter((r) => r.grantId !== row.grantId)
}

// --- Public links ---
function objectName(type: string, id: string): string {
  if (type === 'folder') return allFolders.value.find((f) => f.id === id)?.name ?? t('shares.mine.unknownObject')
  if (type === 'album') return albums.value.find((a) => a.id === id)?.name ?? t('shares.mine.unknownObject')
  return t('shares.mine.unknownObject')
}

function objectTypeLabel(type: string): string {
  if (type === 'folder') return t('shares.mine.typeFolder')
  if (type === 'album') return t('shares.mine.typeAlbum')
  return t('shares.mine.typeAsset')
}

function linkSubtitle(link: ShareLink): string {
  const parts = [
    objectTypeLabel(link.object_type),
    link.expires_at ? t('shares.mine.expiresOn', { date: new Date(link.expires_at).toLocaleDateString() }) : t('shares.mine.noExpiry')
  ]
  if (link.has_password) parts.push(t('shares.mine.passwordActive'))
  parts.push(link.allow_original ? t('shares.mine.originalOn') : t('shares.mine.originalOff'))
  parts.push(t('shares.mine.itemCount', { n: link.item_count }, { plural: link.item_count }))
  return parts.join(' · ')
}

async function copyLink(link: ShareLink) {
  const token = justCreatedTokens.value[link.id]
  if (!token) return
  try {
    await navigator.clipboard.writeText(`${window.location.origin}/s/${token}`)
    toast.show(t('shares.mine.copied'))
  } catch {
    toast.showError(t('shares.mine.copyError'))
  }
}

async function revokeLink(id: string) {
  await revokeShareLink(id)
  links.value = links.value.filter((link) => link.id !== id)
}

// --- Shared folders and albums ---
interface SharedObjectCard {
  type: 'folder' | 'album'
  id: string
  name: string
  itemCount: number
}

const sharedObjectCards = ref<SharedObjectCard[]>([])

async function loadSharedObjectCards() {
  const keys = new Set<string>()
  for (const row of personRows.value) keys.add(`${row.objectType}:${row.objectId}`)
  for (const link of links.value) {
    if (link.object_type === 'folder' || link.object_type === 'album') keys.add(`${link.object_type}:${link.object_id}`)
  }
  sharedObjectCards.value = await Promise.all(
    Array.from(keys).map(async (key) => {
      const [type, id] = key.split(':') as ['folder' | 'album', string]
      if (type === 'album') {
        const members = await fetchAlbumAssets(id).catch(() => [])
        return { type, id, name: objectName('album', id), itemCount: members.length }
      }
      let count = 0
      try {
        let cursor: string | undefined
        do {
          const page = await runSearch({ op: 'folder', id }, cursor)
          count += page.assets.length
          cursor = page.next_cursor
        } while (cursor)
      } catch {
        count = 0
      }
      return { type, id, name: objectName('folder', id), itemCount: count }
    })
  )
}

watch([personRows, links], () => void loadSharedObjectCards())

function openSharedObject(card: SharedObjectCard) {
  void router.push(card.type === 'folder' ? '/folders' : '/albums')
}

// --- "Invite" (admin, permission grant targeting a folder) ---
const inviteOpen = ref(false)
const folders = ref<FolderView[]>([])
const inviteFolderId = ref('')
const subjectType = ref<'user' | 'group'>('user')
const subjectId = ref('')
const role = ref<'viewer' | 'editor'>('viewer')
const inherit = ref(true)
const granting = ref(false)
const inviteGrants = ref<PermissionGrant[]>([])

const subjects = computed(() =>
  subjectType.value === 'user'
    ? users.value.map((u) => ({ id: u.id, label: u.display_name }))
    : groups.value.map((g) => ({ id: g.id, label: g.name }))
)

watch(inviteOpen, async (isOpen) => {
  if (isOpen && folders.value.length === 0) folders.value = await fetchTree().catch(() => [])
})

watch(inviteFolderId, () => {
  explainChain.value = null
  explainGranted.value = null
  void loadInviteGrants()
})

watch(subjectType, () => {
  subjectId.value = ''
})

async function loadInviteGrants() {
  if (!inviteFolderId.value) {
    inviteGrants.value = []
    return
  }
  inviteGrants.value = await fetchPermissions('folder', inviteFolderId.value)
}

async function grant() {
  if (!inviteFolderId.value || !subjectId.value || granting.value) return
  granting.value = true
  try {
    await grantPermission({
      subject_type: subjectType.value,
      subject_id: subjectId.value,
      object_type: 'folder',
      object_id: inviteFolderId.value,
      role: role.value,
      inherit: inherit.value
    })
    await loadInviteGrants()
    await loadPeople()
  } finally {
    granting.value = false
  }
}

async function removeInviteGrant(id: string) {
  await revokePermission(id)
  await loadInviteGrants()
  await loadPeople()
}

const explainUserId = ref('')
const explainChain = ref<ExplainChainLink[] | null>(null)
const explainGranted = ref<boolean | null>(null)

async function explain() {
  if (!inviteFolderId.value || !explainUserId.value) return
  const result = await explainPermission('folder', inviteFolderId.value, explainUserId.value)
  explainGranted.value = result.granted
  explainChain.value = result.chain
}

onMounted(() => {
  void load()
})
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <template v-else>
      <div class="flex gap-1.5 border-b border-border pb-3">
        <button
          type="button"
          class="rounded-full px-3 py-1.5 text-[13px]"
          :class="tab === 'mine' ? 'bg-accent-tint font-semibold text-accent' : 'text-content-muted hover:bg-border/30'"
          @click="tab = 'mine'"
        >
          {{ t('shares.tabs.mine') }}
        </button>
        <button
          type="button"
          class="rounded-full px-3 py-1.5 text-[13px]"
          :class="tab === 'shared' ? 'bg-accent-tint font-semibold text-accent' : 'text-content-muted hover:bg-border/30'"
          @click="tab = 'shared'"
        >
          {{ t('shares.tabs.shared') }}
        </button>
      </div>

      <template v-if="tab === 'mine'">
        <section
          v-if="isAdmin"
          class="mt-6"
        >
          <h2 class="text-[15px] font-bold">
            {{ t('shares.mine.peopleTitle') }}
          </h2>
          <p class="text-sm text-content-muted">
            {{ t('shares.mine.peopleSubtitle') }}
          </p>
          <ul class="mt-3 space-y-1">
            <li
              v-for="row in personRows"
              :key="row.grantId"
              class="flex items-center gap-3 rounded-lg px-2 py-2"
            >
              <Avatar
                :name="subjectName(row)"
                :color="avatarColorFor(row.subjectId)"
              />
              <div class="min-w-0 flex-1">
                <p class="truncate text-[13.5px] font-semibold">
                  {{ subjectName(row) }}
                </p>
                <p
                  v-if="subjectEmail(row)"
                  class="truncate text-[11.5px] text-content-muted"
                >
                  {{ subjectEmail(row) }}
                </p>
                <p
                  v-if="row.inherited"
                  class="truncate text-[11.5px] text-content-muted"
                >
                  {{ t('shares.mine.inheritedFrom', { on: row.objectName }) }}
                </p>
              </div>
              <span class="rounded-full bg-border/40 px-2 py-0.5 text-[11px] text-content-muted">
                {{ roleLabel(row.role) }}
              </span>
              <button
                type="button"
                class="text-[12px] text-danger underline"
                @click="revokeGrant(row)"
              >
                {{ t('shares.revoke') }}
              </button>
            </li>
          </ul>

          <button
            type="button"
            class="mt-3 rounded-lg border border-border px-3 py-1.5 text-[13px]"
            @click="inviteOpen = !inviteOpen"
          >
            {{ t('shares.mine.invite') }}
          </button>

          <div
            v-if="inviteOpen"
            class="mt-4 rounded-lg border border-border p-4"
          >
            <form
              class="grid gap-3 sm:grid-cols-2"
              @submit.prevent="grant"
            >
              <label class="block text-sm">
                {{ t('shares.folder') }}
                <select
                  v-model="inviteFolderId"
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
              v-if="inviteFolderId && inviteGrants.length === 0"
              class="mt-4 text-sm text-content-muted"
            >
              {{ t('shares.grantsEmpty') }}
            </p>
            <ul
              v-else-if="inviteGrants.length > 0"
              data-testid="shares-grants"
              class="mt-4 space-y-2"
            >
              <li
                v-for="grantRow in inviteGrants"
                :key="grantRow.id"
                class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
              >
                <p class="text-sm">
                  {{ grantRow.subject_id }} · {{ grantRow.role }}
                </p>
                <button
                  class="ml-4 text-sm text-danger underline"
                  type="button"
                  @click="removeInviteGrant(grantRow.id)"
                >
                  {{ t('shares.revoke') }}
                </button>
              </li>
            </ul>

            <h3 class="mt-6 text-base font-semibold">
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
          </div>
        </section>

        <section class="mt-8">
          <h2 class="text-[15px] font-bold">
            {{ t('shares.linksTitle') }}
          </h2>
          <p class="text-sm text-content-muted">
            {{ t('shares.mine.linksSubtitle') }}
          </p>
          <p
            v-if="links.length === 0"
            class="mt-4 text-sm text-content-muted"
          >
            {{ t('shares.empty') }}
          </p>
          <ul
            v-else
            class="mt-3 space-y-2"
          >
            <li
              v-for="link in links"
              :key="link.id"
              class="flex items-center gap-3 rounded-lg border border-border px-4 py-3"
            >
              <span class="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-lg bg-chip-bg">
                <svg
                  viewBox="0 0 24 24"
                  width="14"
                  height="14"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  aria-hidden="true"
                >
                  <path d="M10 13a5 5 0 0 0 7.07 0l1.93-1.93a5 5 0 0 0-7.07-7.07L10.5 5.5" />
                  <path d="M14 11a5 5 0 0 0-7.07 0L5 12.93a5 5 0 0 0 7.07 7.07L13.5 18.5" />
                </svg>
              </span>
              <div class="min-w-0 flex-1">
                <p class="truncate text-[13px] font-semibold">
                  {{ objectName(link.object_type, link.object_id) }}
                </p>
                <p class="truncate text-[11.5px] text-content-muted">
                  {{ linkSubtitle(link) }}
                </p>
              </div>
              <button
                v-if="justCreatedTokens[link.id]"
                type="button"
                class="rounded-lg border border-border px-2.5 py-1 text-[12px]"
                @click="copyLink(link)"
              >
                {{ t('shares.mine.copy') }}
              </button>
              <button
                type="button"
                class="rounded-lg border border-danger px-2.5 py-1 text-[12px] text-danger hover:bg-danger/10"
                @click="revokeLink(link.id)"
              >
                {{ t('shares.revoke') }}
              </button>
            </li>
          </ul>
        </section>

        <section
          v-if="sharedObjectCards.length > 0"
          class="mt-8"
        >
          <h2 class="text-[15px] font-bold">
            {{ t('shares.mine.foldersTitle') }}
          </h2>
          <div class="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
            <button
              v-for="card in sharedObjectCards"
              :key="`${card.type}:${card.id}`"
              type="button"
              class="overflow-hidden rounded-lg border border-border text-left"
              @click="openSharedObject(card)"
            >
              <div class="relative h-[90px] w-full bg-chip-bg">
                <span class="absolute right-1.5 top-1.5 rounded bg-black/55 px-1.5 py-0.5 text-[10px] font-bold text-white">
                  {{ t('shares.mine.sharedBadge') }}
                </span>
              </div>
              <div class="px-2 py-1.5">
                <p class="truncate text-[13.5px] font-bold">
                  {{ card.name }}
                </p>
                <p class="text-[11.5px] text-content-muted">
                  {{ t('shares.mine.itemCount', { n: card.itemCount }, { plural: card.itemCount }) }} · {{ objectTypeLabel(card.type) }}
                </p>
              </div>
            </button>
          </div>
        </section>
      </template>

      <template v-else>
        <template v-if="sharedWithMe.length > 0">
          <h2 class="mt-6 text-[15px] font-bold">
            {{ t('shares.shared.title') }}
          </h2>
          <div class="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
            <div
              v-for="item in sharedWithMe"
              :key="`${item.object_type}:${item.object_id}`"
              class="overflow-hidden rounded-lg border border-border"
            >
              <div class="relative h-[90px] w-full bg-chip-bg">
                <span class="absolute right-1.5 top-1.5 rounded bg-black/55 px-1.5 py-0.5 text-[10px] font-bold text-white">
                  {{ t('shares.shared.fromOwner', { owner: item.owner_name }) }}
                </span>
              </div>
              <div class="px-2 py-1.5">
                <p class="truncate text-[13.5px] font-bold">
                  {{ item.name }}
                </p>
                <p class="text-[11.5px] text-content-muted">
                  {{
                    t('shares.shared.subtitle', {
                      type: objectTypeLabel(item.object_type),
                      n: item.item_count,
                      role: roleLabel(item.role)
                    })
                  }}
                </p>
              </div>
            </div>
          </div>
        </template>
        <div
          v-else
          class="mt-10 flex flex-col items-center gap-1 text-center"
        >
          <p class="text-sm font-semibold">
            {{ t('shares.shared.emptyTitle') }}
          </p>
          <p class="max-w-sm text-sm text-content-muted">
            {{ t('shares.shared.emptySubtitle') }}
          </p>
        </div>
      </template>
    </template>
  </main>
</template>

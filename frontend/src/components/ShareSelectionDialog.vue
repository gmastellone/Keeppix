<script setup lang="ts">
// "Share selection" dialog: shares a selection of photos without sharing
// the entire folder/album they come from — with people already known, or
// via a new public link scoped to the selection.
//
// The backend has no (and never had an) `object_type` for "an arbitrary
// set of photos": verified by reading `crates/keeppix-db/src/share_links.rs`
// and `permissions.rs` in full — only `folder`/`album`/`asset` exist
// anywhere (SQL row, route validation, `item_counts`). The same constraint
// had already been explicitly declared as the reason "Share" didn't yet
// appear in the selection bar (see `LibrarySelectionActions.vue`) nor in
// the lightbox (`AssetViewer.vue`).
//
// No need to extend the backend: an "arbitrary set of photos" is exactly
// what an **album** already represents, with permissions and public links
// already fully built. This creates (on first real use, not when the
// dialog opens) a hidden album containing only the selection, and shares
// *that* — the prototype's "Manual selection" becomes, in the real system,
// an auto-generated album: the same promise ("you're not sharing the
// entire folder/album they come from"), a real mechanism instead of a
// nonexistent `object_type`.
//
// The "People" section (list of who can be invited) requires `GET
// /users`, which the backend reserves for admins
// (`crates/keeppix-api/src/routes/users.rs`, `AdminAuth` on every route)
// — no alternative route exists for a regular user to list the instance's
// other accounts. For a non-admin, the public link remains entirely
// usable regardless, since it doesn't require that list.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { addAssets, createAlbum } from '@/api/albums'
import { grantPermission, revokePermission } from '@/api/permissions'
import { createShareLink } from '@/api/shares'
import { fetchUsers, type UserSummary } from '@/api/users'
import { avatarColorFor } from '@/design/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import Avatar from './ui/Avatar.vue'
import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ assetIds: string[] }>()
const emit = defineEmits<{ created: [] }>()

const { t } = useI18n()
const session = useSessionStore()
const toast = useToastStore()

const isAdmin = computed(() => session.user?.role === 'admin')
const people = ref<UserSummary[]>([])
const grants = ref<Record<string, string>>({})
const pending = ref<Set<string>>(new Set())
const creatingLink = ref(false)
const albumId = ref<string | null>(null)
const firstRowEl = ref<HTMLButtonElement | null>(null)
const createLinkEl = ref<HTMLButtonElement | null>(null)

async function loadPeople() {
  if (!isAdmin.value) {
    people.value = []
    return
  }
  try {
    people.value = (await fetchUsers()).filter((user) => user.id !== session.user?.id)
  } catch {
    people.value = []
  }
}

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      albumId.value = null
      grants.value = {}
      void loadPeople()
    }
  },
  { immediate: true }
)

const initialFocus = computed(() => firstRowEl.value ?? createLinkEl.value ?? null)

/** Creates the hidden album for this selection, if it doesn't exist yet —
 * lazily: opening the dialog without touching anything creates nothing. */
async function ensureAlbum(): Promise<string> {
  if (albumId.value) return albumId.value
  const now = new Date()
  const name = t('shares.dialog.autoAlbumName', {
    date: now.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  })
  const album = await createAlbum(name)
  await addAssets(album.id, props.assetIds)
  albumId.value = album.id
  return album.id
}

async function togglePerson(person: UserSummary) {
  if (pending.value.has(person.id)) return
  pending.value = new Set(pending.value).add(person.id)
  try {
    const existingGrantId = grants.value[person.id]
    if (existingGrantId) {
      await revokePermission(existingGrantId)
      const next = { ...grants.value }
      delete next[person.id]
      grants.value = next
      toast.show(t('shares.dialog.revoked', { name: person.display_name }))
    } else {
      const id = await ensureAlbum()
      const { id: grantId } = await grantPermission({
        subject_type: 'user',
        subject_id: person.id,
        object_type: 'album',
        object_id: id,
        role: 'viewer',
        inherit: false
      })
      grants.value = { ...grants.value, [person.id]: grantId }
      toast.show(t('shares.dialog.granted', { name: person.display_name }))
    }
  } catch {
    toast.showError(t('shares.dialog.error'))
  } finally {
    const rest = new Set(pending.value)
    rest.delete(person.id)
    pending.value = rest
  }
}

async function createLink() {
  if (creatingLink.value) return
  creatingLink.value = true
  try {
    const id = await ensureAlbum()
    const { token } = await createShareLink({ object_type: 'album', object_id: id })
    const url = `${window.location.origin}/s/${token}`
    try {
      await navigator.clipboard.writeText(url)
    } catch {
      // No clipboard access (permission denied, insecure context): the
      // link is still created and visible in Shares — only the automatic
      // copy is skipped, not the whole operation.
    }
    toast.show(t('shares.dialog.linkCreated'))
    emit('created')
    open.value = false
  } catch {
    toast.showError(t('shares.dialog.error'))
  } finally {
    creatingLink.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('shares.dialog.title', { n: assetIds.length }, { plural: assetIds.length })"
    :description="t('shares.dialog.subtitle')"
    :initial-focus="initialFocus"
  >
    <template v-if="isAdmin">
      <h3 class="text-[12.5px] font-semibold text-content-muted">
        {{ t('shares.dialog.peopleTitle') }}
      </h3>
      <div class="mt-1 max-h-[260px] space-y-1 overflow-y-auto">
        <button
          v-for="(person, index) in people"
          :key="person.id"
          :ref="(el) => { if (index === 0) firstRowEl = el as HTMLButtonElement }"
          type="button"
          role="switch"
          :aria-checked="Boolean(grants[person.id])"
          :aria-label="person.display_name"
          :disabled="pending.has(person.id)"
          class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-border/20
                 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          @click="togglePerson(person)"
        >
          <Avatar
            :name="person.display_name"
            :color="avatarColorFor(person.id)"
            aria-hidden="true"
          />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-[13px] font-semibold text-content">{{ person.display_name }}</span>
            <span class="block text-[11.5px] text-content-muted">{{ t('shares.dialog.role') }}</span>
          </span>
          <span
            class="relative h-5 w-9 shrink-0 rounded-full transition-colors"
            :style="{ transitionDuration: 'var(--duration-arrow)' }"
            :class="grants[person.id] ? 'bg-accent' : 'bg-border'"
          >
            <span
              class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
              :style="{ left: grants[person.id] ? '18px' : '2px', transitionDuration: 'var(--duration-arrow)' }"
            />
          </span>
        </button>
      </div>
    </template>

    <h3 class="mt-4 text-[12.5px] font-semibold text-content-muted">
      {{ t('shares.dialog.linkSectionTitle') }}
    </h3>
    <p class="mt-0.5 text-[11.5px] text-content-muted">
      {{ t('shares.dialog.linkSectionSubtitle') }}
    </p>
    <button
      ref="createLinkEl"
      type="button"
      :disabled="creatingLink"
      class="mt-2 w-full rounded-lg border border-border py-2 text-center text-[13px] font-semibold hover:bg-border/20"
      @click="createLink"
    >
      {{ t('shares.dialog.createLink') }}
    </button>

    <div class="mt-4 flex justify-end">
      <button
        type="button"
        class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold"
        @click="open = false"
      >
        {{ t('shares.dialog.done') }}
      </button>
    </div>
  </Dialog>
</template>

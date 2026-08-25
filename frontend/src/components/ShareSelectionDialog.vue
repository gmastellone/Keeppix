<script setup lang="ts">
// Task 11 (1/N), §30 "Dialog 'Condividi selezione'": condivide una
// selezione di foto senza condividere l'intera cartella/album di
// provenienza — con persone già note, o via un nuovo link pubblico
// scoped alla selezione.
//
// Il backend non ha (e non ha mai avuto) un `object_type` per "una
// selezione arbitraria di foto": verificato leggendo per intero
// `crates/keeppix-db/src/share_links.rs` e `permissions.rs` — solo
// `folder`/`album`/`asset` esistono ovunque (riga SQL, validazione della
// rotta, `item_counts`). Lo stesso vincolo era già stato dichiarato
// esplicitamente al Task 7 (2/N) come motivo per cui "Condividi" non
// compariva ancora nella barra di selezione (vedi `LibrarySelectionActions
// .vue`) né nel lightbox (`AssetViewer.vue`).
//
// Non serve estendere il backend: un "insieme arbitrario di foto" è
// esattamente ciò che un **album** già rappresenta, con permessi e link
// pubblici già completi. Qui si crea (al primo uso reale, non
// all'apertura del dialog) un album nascosto che contiene solo la
// selezione, e si condivide *quello* — "Selezione manuale" del mockup
// diventa, nel sistema reale, un album auto-generato: stessa promessa
// ("non condividi l'intera cartella/album di provenienza"), meccanismo
// reale invece di un `object_type` che non esiste.
//
// La sezione "Persone" (elenco di chi si può invitare) richiede
// `GET /users`, che il backend riserva agli admin
// (`crates/keeppix-api/src/routes/users.rs`, `AdminAuth` su ogni rotta)
// — nessuna rotta alternativa esiste per un utente normale per elencare
// gli altri account dell'istanza. Per chi non è admin resta comunque
// interamente utilizzabile il link pubblico, che non richiede quell'elenco.
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

/** Crea l'album nascosto per questa selezione, se non esiste ancora —
 * pigro: aprire il dialog senza toccare nulla non crea nulla. */
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
      // Niente clipboard (permesso negato, contesto non sicuro): il link
      // resta comunque creato e visibile in Condivisioni — solo la copia
      // automatica salta, non l'intera operazione.
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

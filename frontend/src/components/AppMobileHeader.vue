<script setup lang="ts">
// Mobile header.
//
// Title per route: `ROUTE_TITLE_KEYS` (`src/nav/routeTitles.ts`), shared
// with AppTopbar — the exact same text for every route currently covered,
// not a second diverging copy. `/more` is **not** in that map: the mockup
// explicitly says the `library`/`folders` views "only exist in the mobile
// shell" and stay at an empty breadcrumb when going back to desktop —
// here, mobile-only, the "More" title is added separately.
//
// Back arrow: the mockup has three priority branches (open album detail →
// Albums grid; culling/bulkEdit → Photos; otherwise → More). The first
// branch is reachable: `/albums/:id` is the first dynamic route with an
// "open" state observable from outside the view (the same gap already
// declared for AppSidebar, still true for folders/culling — neither
// exposes that state yet).
//
// Culling funnel button: badge from the real data already used by
// AppSidebar (`shell.badges.culling`), not a placeholder.
//
// Account menu: the mockup lists Profile/Settings/Sign out — all three
// present, same order and same link as AppSidebar's desktop account menu.
//
// Upload `+`: the mockup shows it on
// `['foto','preferiti','album','libreria']` (`MOBILE_UPLOAD_VIEWS`) — here
// only `/`/`/albums`/`/more`, the only three of that list with a real view
// (Favorites doesn't exist, same gap as AppSidebar/MoreView).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import Popover from '@/components/ui/Popover.vue'
import { UPLOAD_ACCEPT, useUploadPicker } from '@/composables/useUploadPicker'
import { activeAlbumName, activeCullingLotName, activePersonName, ROUTE_TITLE_KEYS } from '@/nav/routeTitles'
import { useAvatarColorStore } from '@/stores/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const shell = useShellStore()
const avatarColor = useAvatarColorStore()
const inputEl = ref<HTMLInputElement | null>(null)
const { open: openPicker, onChange } = useUploadPicker(inputEl)

onMounted(() => {
  if (!shell.loaded) void shell.load()
})

const ROOT_ROUTES = new Set(['/', '/search', '/albums', '/more'])
const BACK_TO_FOTO = new Set(['/culling', '/batch-edit'])
const UPLOAD_VISIBLE_ROUTES = new Set(['/', '/albums', '/more'])

const showBack = computed(() => !ROOT_ROUTES.has(route.path))

const title = computed(() => {
  if (route.path.startsWith('/albums/') && activeAlbumName.value) return activeAlbumName.value
  // "On mobile the title is 'People' or the person's name" — without a
  // name, the flat title from the map below (`persons.title`) is used.
  if (route.path.startsWith('/persons/') && activePersonName.value) return activePersonName.value
  // "Mobile header: the title is the lot's name".
  if (route.path.startsWith('/culling/') && activeCullingLotName.value) return activeCullingLotName.value
  const key = ROUTE_TITLE_KEYS[route.path] ?? (route.path.startsWith('/persons/') ? 'persons.title' : undefined)
  if (key) return t(key)
  if (route.path === '/more') return t('nav.more')
  return t('app.name')
})

function goBack() {
  if (route.path.startsWith('/albums/')) {
    void router.push('/albums')
    return
  }
  if (route.path.startsWith('/persons/')) {
    void router.push('/persons')
    return
  }
  // "The back arrow goes directly to state.view='foto' — not to the lots
  // grid", even from an open lot.
  if (route.path.startsWith('/culling/')) {
    void router.push('/')
    return
  }
  void router.push(BACK_TO_FOTO.has(route.path) ? '/' : '/more')
}

const accountMenuOpen = ref(false)

async function signOut() {
  await session.logout()
  await router.push('/login')
}
</script>

<template>
  <div class="flex h-[52px] shrink-0 items-center justify-between border-b border-border px-3.5">
    <div class="flex min-w-0 items-center gap-1">
      <button
        v-if="showBack"
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
        :aria-label="t('common.back')"
        @click="goBack"
      >
        ←
      </button>
      <span class="truncate text-[15.5px] font-bold">{{ title }}</span>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <button
        v-if="UPLOAD_VISIBLE_ROUTES.has(route.path)"
        type="button"
        class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
        :aria-label="t('upload.uploadTooltip')"
        @click="openPicker"
      >
        +
      </button>
      <RouterLink
        v-if="route.path === '/'"
        to="/culling"
        class="relative rounded-lg px-2 py-1.5 text-sm font-semibold text-content-muted hover:bg-border/40"
        :aria-label="t('culling.entry')"
      >
        {{ t('culling.entry') }}
        <span
          v-if="shell.badges.culling > 0"
          class="absolute -right-1.5 -top-1.5 min-w-[16px] rounded-full bg-danger px-1 text-center text-[10px] font-bold text-white"
        >
          {{ shell.badges.culling }}
        </span>
      </RouterLink>
      <Popover
        v-if="session.user"
        v-model:open="accountMenuOpen"
        side="bottom"
        align="end"
      >
        <template #trigger>
          <button
            type="button"
            class="flex h-7 w-7 items-center justify-center rounded-full"
          >
            <Avatar
              :name="session.user.display_name"
              :color="avatarColor.hex"
            />
          </button>
        </template>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-border/30"
          @click="accountMenuOpen = false; router.push('/profile')"
        >
          {{ t('profile.entry') }}
        </button>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-border/30"
          @click="accountMenuOpen = false; router.push('/settings')"
        >
          {{ t('settings.entry') }}
        </button>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] text-danger hover:bg-border/30"
          @click="signOut"
        >
          {{ t('home.logout') }}
        </button>
      </Popover>
      <input
        ref="inputEl"
        type="file"
        multiple
        :accept="UPLOAD_ACCEPT"
        class="hidden"
        :aria-hidden="true"
        tabindex="-1"
        @change="onChange"
      >
    </div>
  </div>
</template>

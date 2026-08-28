<script setup lang="ts">
import { defineAsyncComponent, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import AppShell from '@/components/ui/AppShell.vue'
import AppMobileHeader from '@/components/AppMobileHeader.vue'
import AppMobileTabbar from '@/components/AppMobileTabbar.vue'
import AppSidebar from '@/components/AppSidebar.vue'
import AppTopbar from '@/components/AppTopbar.vue'
import Button from '@/components/ui/Button.vue'
import ToastHost from '@/components/ui/ToastHost.vue'
import UploadDropVeil from '@/components/UploadDropVeil.vue'
import UploadQueueStrip from '@/components/UploadQueueStrip.vue'
import { useAvatarColorStore } from '@/stores/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useThemeStore } from '@/stores/theme'

const { t } = useI18n()
const session = useSessionStore()
const theme = useThemeStore()
const avatarColor = useAvatarColorStore()

// Theme lives in the server-side preferences, so it requires a session —
// loaded/reset from a single place observing `session.user`, instead of
// from every action that could change it (bootstrap/login/setup/logout).
//
// The avatar color follows the same single point, but reads `localStorage`
// (keyed by `user.id`, never server preferences — see
// `stores/avatarColor.ts`), so `load()` is synchronous: no `void`.
watch(
  () => session.user,
  (user) => {
    if (user) {
      void theme.load()
      avatarColor.load(user.id)
    } else {
      theme.reset()
      avatarColor.reset()
    }
  },
  { immediate: true }
)

// Deferred import: the upload panel is a global overlay that lives outside
// the router, but without `defineAsyncComponent` it would still end up in
// the initial bundle — even for a first load by someone who never uploads
// anything.
const UploadPanel = defineAsyncComponent(() => import('@/components/UploadPanel.vue'))
</script>

<template>
  <main
    v-if="session.unavailable"
    class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-4 p-6"
  >
    <h1 class="text-xl font-semibold">
      {{ t('common.unavailable') }}
    </h1>
    <p class="text-content-muted">
      {{ t('common.unavailableHint') }}
    </p>
    <Button @click="session.retryBootstrap()">
      {{ t('common.retry') }}
    </Button>
  </main>
  <!-- The real shell (AppShell) replaces a bare <RouterView>, for both
       desktop and mobile. -->
  <AppShell
    v-else
    class="h-full"
  >
    <template #sidebar>
      <AppSidebar />
    </template>
    <template #topbar>
      <AppTopbar />
    </template>
    <template #mobile-header>
      <AppMobileHeader />
    </template>
    <template #mobile-tabbar>
      <!-- "A strip above the tab bar" — above, not inside it. -->
      <UploadQueueStrip />
      <AppMobileTabbar />
    </template>
    <RouterView />
    <!-- Main entry point for uploads: the drag/drop listeners need to be
         live from the start, not behind a deferred import like
         UploadPanel — at rest (`v-if="dragging"` inside, always false) it
         doesn't add a single visible pixel. -->
    <UploadDropVeil />
  </AppShell>
  <UploadPanel v-if="!session.unavailable" />
  <ToastHost />
</template>

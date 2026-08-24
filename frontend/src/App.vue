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

// Fase 11 Task 14 (1/N), §60.1: il tema vive nelle preferenze del server,
// quindi richiede una sessione — caricato/azzerato in un unico punto
// osservando `session.user`, invece che da ogni azione che potrebbe farlo
// cambiare (bootstrap/login/setup/logout).
//
// Task 14 (2/N), §61.2: il colore avatar segue lo stesso punto unico, ma
// legge `localStorage` (per `user.id`, mai le preferenze server — vedi
// `stores/avatarColor.ts`), quindi `load()` è sincrono: nessun `void`.
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

// Import differito: il pannello di upload è un overlay globale che vive
// fuori dal router, ma senza `defineAsyncComponent` finirebbe comunque nel
// bundle iniziale — anche il primo caricamento di chi non fa mai un upload.
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
  <!-- Fase 11 Task 6 (3/N, 8/N): l'impalcatura reale (AppShell, Task 2)
       prende il posto del solo <RouterView>, sia desktop che mobile. -->
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
      <!-- §6.1: "una fascia sopra la tab bar" — sopra, non dentro. -->
      <UploadQueueStrip />
      <AppMobileTabbar />
    </template>
    <RouterView />
    <!-- Porta d'ingresso principale del caricamento (§3.1): i listener
         devono essere vivi da subito, non dietro un import differito
         come UploadPanel — a riposo (`v-if="dragging"` dentro, sempre
         false) non aggiunge un solo pixel visibile. -->
    <UploadDropVeil />
  </AppShell>
  <UploadPanel v-if="!session.unavailable" />
  <ToastHost />
</template>

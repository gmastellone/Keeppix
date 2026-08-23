<script setup lang="ts">
import { defineAsyncComponent } from 'vue'
import { useI18n } from 'vue-i18n'

import AppShell from '@/components/ui/AppShell.vue'
import AppMobileHeader from '@/components/AppMobileHeader.vue'
import AppMobileTabbar from '@/components/AppMobileTabbar.vue'
import AppSidebar from '@/components/AppSidebar.vue'
import AppTopbar from '@/components/AppTopbar.vue'
import Button from '@/components/ui/Button.vue'
import ToastHost from '@/components/ui/ToastHost.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const session = useSessionStore()

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
      <AppMobileTabbar />
    </template>
    <RouterView />
  </AppShell>
  <UploadPanel v-if="!session.unavailable" />
  <ToastHost />
</template>

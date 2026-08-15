<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import Button from '@/components/ui/Button.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const session = useSessionStore()
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
  <RouterView v-else />
</template>

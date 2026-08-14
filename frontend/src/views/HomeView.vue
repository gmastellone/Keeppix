<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

async function signOut() {
  await session.logout()
  await router.push('/login')
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-6 p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('home.greeting', { name: session.user?.display_name ?? '' }) }}
    </h1>
    <div class="max-w-xs">
      <Button @click="signOut">
        {{ t('home.logout') }}
      </Button>
    </div>
  </main>
</template>

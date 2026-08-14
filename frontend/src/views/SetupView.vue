<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const displayName = ref('')
const username = ref('')
const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

/** Il backend restituisce codici stabili: la traduzione avviene qui. */
function messageFor(e: unknown): string {
  if (!(e instanceof ApiProblem)) return t('common.unexpectedError')
  const known: Record<string, string> = {
    'keeppix/invalid-username': t('setup.errors.invalidUsername'),
    'keeppix/invalid-password': t('setup.errors.invalidPassword'),
    'keeppix/already-initialised': t('setup.errors.alreadyInitialised')
  }
  return known[e.type] ?? t('common.unexpectedError')
}

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.setup({
      username: username.value,
      display_name: displayName.value,
      email: email.value || undefined,
      password: password.value
    })
    await router.push('/')
  } catch (e) {
    error.value = messageFor(e)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <header class="flex flex-col gap-1">
      <h1 class="text-2xl font-semibold">
        {{ t('setup.title') }}
      </h1>
      <p class="text-sm text-content-muted">
        {{ t('setup.subtitle') }}
      </p>
    </header>

    <form
      class="flex flex-col gap-4"
      @submit.prevent="submit"
    >
      <TextField
        v-model="displayName"
        :label="t('setup.displayName')"
        autocomplete="name"
        required
      />
      <TextField
        v-model="username"
        :label="t('setup.username')"
        autocomplete="username"
        required
      />
      <TextField
        v-model="email"
        :label="t('setup.email')"
        type="email"
        autocomplete="email"
      />
      <TextField
        v-model="password"
        :label="t('setup.password')"
        :hint="t('setup.passwordHint')"
        type="password"
        autocomplete="new-password"
        required
      />
      <Alert
        v-if="error"
        :message="error"
      />
      <Button
        type="submit"
        :loading="loading"
      >
        {{ t('setup.submit') }}
      </Button>
    </form>
  </main>
</template>

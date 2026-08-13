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

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.login(username.value, password.value)
    await router.push('/')
  } catch (e) {
    error.value =
      e instanceof ApiProblem && e.type === 'keeppix/invalid-credentials'
        ? t('login.errors.invalidCredentials')
        : t('common.unexpectedError')
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('login.title') }}
    </h1>
    <form
      class="flex flex-col gap-4"
      @submit.prevent="submit"
    >
      <TextField
        v-model="username"
        :label="t('login.username')"
        autocomplete="username"
        required
      />
      <TextField
        v-model="password"
        :label="t('login.password')"
        type="password"
        autocomplete="current-password"
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
        {{ t('login.submit') }}
      </Button>
    </form>
  </main>
</template>

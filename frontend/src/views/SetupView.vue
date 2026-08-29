<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

import LibraryStep from './setup/LibraryStep.vue'
import ScanStep from './setup/ScanStep.vue'

const { t } = useI18n()
const session = useSessionStore()

type WizardStep = 'admin' | 'library' | 'scan'

const step = ref<WizardStep>('admin')
const libraryId = ref<string | null>(null)

const displayName = ref('')
const username = ref('')
const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

/** The backend returns stable error codes; the translation happens here. */
function messageFor(e: unknown): string {
  if (!(e instanceof ApiProblem)) return t('common.unexpectedError')
  const known: Record<string, string> = {
    'keeppix/invalid-username': t('setup.errors.invalidUsername'),
    'keeppix/invalid-password': t('setup.errors.invalidPassword'),
    'keeppix/already-initialised': t('setup.errors.alreadyInitialised')
  }
  return known[e.type] ?? t('common.unexpectedError')
}

async function submitAdmin() {
  error.value = ''
  loading.value = true
  try {
    await session.setup({
      username: username.value,
      display_name: displayName.value,
      email: email.value || undefined,
      password: password.value
    })
    step.value = 'library'
  } catch (e) {
    error.value = messageFor(e)
  } finally {
    loading.value = false
  }
}

function onLibraryCreated(id: string) {
  libraryId.value = id
  step.value = 'scan'
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <header
      v-if="step === 'admin'"
      class="flex flex-col gap-1"
    >
      <h1 class="text-2xl font-semibold">
        {{ t('setup.title') }}
      </h1>
      <p class="text-sm text-content-muted">
        {{ t('setup.subtitle') }}
      </p>
    </header>

    <form
      v-if="step === 'admin'"
      class="flex flex-col gap-4"
      @submit.prevent="submitAdmin"
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

    <LibraryStep
      v-else-if="step === 'library'"
      @created="onLibraryCreated"
    />

    <ScanStep
      v-else-if="step === 'scan' && libraryId"
      :library-id="libraryId"
    />
  </main>
</template>

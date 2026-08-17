<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { ApiProblem } from '@/api/client'
import { createLibrary, previewLibraryPath } from '@/api/libraries'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'

const emit = defineEmits<{ created: [libraryId: string] }>()

const { t } = useI18n()

const name = ref('Photos')
const rootPath = ref('/photos')
const error = ref('')
const previewLoading = ref(false)
const createLoading = ref(false)
const preview = ref<{ total: number; extensions: Record<string, number> } | null>(null)

function messageForPreview(e: unknown): string {
  if (e instanceof ApiProblem && e.type === 'keeppix/path-not-allowed') {
    return t('setup.library.errors.pathNotAllowed')
  }
  return t('setup.library.errors.previewFailed')
}

function messageForCreate(e: unknown): string {
  if (e instanceof ApiProblem && e.type === 'keeppix/path-not-allowed') {
    return t('setup.library.errors.pathNotAllowed')
  }
  return t('setup.library.errors.createFailed')
}

async function runPreview() {
  error.value = ''
  previewLoading.value = true
  preview.value = null
  try {
    preview.value = await previewLibraryPath(rootPath.value)
  } catch (e) {
    error.value = messageForPreview(e)
  } finally {
    previewLoading.value = false
  }
}

async function submit() {
  if (!preview.value) return
  error.value = ''
  createLoading.value = true
  try {
    const library = await createLibrary({ name: name.value, root_path: rootPath.value })
    emit('created', library.id)
  } catch (e) {
    error.value = messageForCreate(e)
  } finally {
    createLoading.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex flex-col gap-1">
      <h2 class="text-xl font-semibold">
        {{ t('setup.library.title') }}
      </h2>
      <p class="text-sm text-content-muted">
        {{ t('setup.library.subtitle') }}
      </p>
    </header>

    <TextField
      v-model="name"
      :label="t('setup.library.name')"
      autocomplete="off"
      required
    />
    <TextField
      v-model="rootPath"
      :label="t('setup.library.path')"
      :hint="t('setup.library.pathHint')"
      autocomplete="off"
      required
    />

    <Button
      type="button"
      :loading="previewLoading"
      @click="runPreview"
    >
      {{ t('setup.library.preview') }}
    </Button>

    <div
      v-if="preview"
      class="rounded-lg border border-border bg-surface-elevated p-3 text-sm"
    >
      <p class="font-medium">
        {{ t('setup.library.previewTotal', { count: preview.total }) }}
      </p>
      <ul class="mt-2 space-y-1 text-content-muted">
        <li
          v-for="(count, ext) in preview.extensions"
          :key="ext"
        >
          {{ t('setup.library.previewExtension', { ext, count }) }}
        </li>
      </ul>
    </div>

    <Alert
      v-if="error"
      :message="error"
    />

    <Button
      type="button"
      :disabled="!preview"
      :loading="createLoading"
      @click="submit"
    >
      {{ t('setup.library.continue') }}
    </Button>
  </section>
</template>

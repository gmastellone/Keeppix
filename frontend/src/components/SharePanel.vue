<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { createShareLink } from '@/api/shares'

const props = defineProps<{
  resourceType: string
  resourceId: string
}>()

const emit = defineEmits<{
  close: []
}>()

const { t } = useI18n()

const share = ref<{ token: string } | null>(null)
const creating = ref(false)

async function create() {
  creating.value = true
  try {
    share.value = await createShareLink({
      object_type: props.resourceType,
      object_id: props.resourceId
    })
  } finally {
    creating.value = false
  }
}

function shareUrl(): string {
  if (!share.value) return ''
  return `${window.location.origin}/s/${share.value.token}`
}
</script>

<template>
  <aside class="rounded-lg border border-border bg-surface-elevated p-4">
    <div class="flex items-center justify-between">
      <h2 class="font-medium">
        {{ t('sharePanel.title') }}
      </h2>
      <button
        class="text-sm underline"
        @click="emit('close')"
      >
        {{ t('sharePanel.close') }}
      </button>
    </div>
    <div
      v-if="share"
      class="mt-3"
    >
      <input
        class="w-full rounded border border-border bg-surface px-2 py-1 text-sm"
        :value="shareUrl()"
        readonly
      >
    </div>
    <button
      v-else
      class="mt-3 rounded-lg bg-accent px-4 py-2 text-sm text-white"
      :disabled="creating"
      @click="create"
    >
      {{ t('sharePanel.createLink') }}
    </button>
  </aside>
</template>

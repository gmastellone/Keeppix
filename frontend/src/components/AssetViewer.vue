<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { TimelineAsset } from '@/api/timeline'

const props = defineProps<{ asset: TimelineAsset }>()
const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const info = ref(false)

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
  if (e.key === 'i') info.value = !info.value
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))

const src = props.asset.content_hash
  ? `/media/preview/${props.asset.content_hash}`
  : `/media/original/${props.asset.id}`
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex bg-black/90"
    role="dialog"
    :aria-label="t('viewer.title')"
    @click.self="emit('close')"
  >
    <img
      :src="src"
      :alt="asset.filename"
      class="m-auto max-h-full max-w-full object-contain"
    >
    <button
      class="absolute top-3 right-3 rounded bg-white/20 px-3 py-1 text-white"
      @click="emit('close')"
    >
      {{ t('viewer.close') }}
    </button>
    <aside
      v-if="info"
      class="absolute right-0 top-0 h-full w-72 overflow-auto bg-surface p-4 text-sm"
    >
      <p>{{ asset.filename }}</p>
      <p v-if="asset.taken_at_utc">
        {{ asset.taken_at_utc }}
      </p>
      <p v-if="asset.width && asset.height">
        {{ asset.width }} × {{ asset.height }}
      </p>
    </aside>
  </div>
</template>

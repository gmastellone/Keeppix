<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'

// Nome multi-parola esplicito: il file si chiama `Filmstrip.vue` per
// combaciare col piano, ma `vue/multi-word-component-names` vuole un nome
// di componente di più parole per non confondersi con futuri elementi HTML.
defineOptions({ name: 'CullingFilmstrip' })

defineProps<{
  assets: TimelineAsset[]
  currentId?: string
}>()
const emit = defineEmits<{ select: [id: string] }>()
const { t } = useI18n()

function thumbSrc(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}
</script>

<template>
  <div
    class="flex gap-1.5 overflow-x-auto border-t border-b border-border bg-black/40 px-10 py-2.5"
    role="listbox"
    :aria-label="t('culling.filmstrip.label')"
  >
    <button
      v-for="asset in assets"
      :key="asset.id"
      type="button"
      role="option"
      class="relative h-[58px] w-[58px] shrink-0 overflow-hidden rounded-md border-2"
      :class="asset.id === currentId ? 'border-accent' : 'border-transparent'"
      :aria-selected="asset.id === currentId"
      :aria-label="asset.filename"
      @click="emit('select', asset.id)"
    >
      <img
        v-if="thumbSrc(asset)"
        :src="thumbSrc(asset)"
        :alt="asset.filename"
        class="h-full w-full object-cover"
      >
    </button>
  </div>
</template>

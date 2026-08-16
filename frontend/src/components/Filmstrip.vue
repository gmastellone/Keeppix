<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { AssetFlags } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'

// Nome multi-parola esplicito: il file si chiama `Filmstrip.vue` per
// combaciare col piano, ma `vue/multi-word-component-names` vuole un nome
// di componente di più parole per non confondersi con futuri elementi HTML.
defineOptions({ name: 'CullingFilmstrip' })

defineProps<{
  assets: TimelineAsset[]
  currentId?: string
  flagsFor: (id: string) => AssetFlags
}>()
const emit = defineEmits<{ select: [id: string] }>()
const { t } = useI18n()

function thumbSrc(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? `/media/thumb/${asset.content_hash}` : undefined
}
</script>

<template>
  <div
    class="flex gap-1 overflow-x-auto bg-black/40 p-2"
    role="listbox"
    :aria-label="t('culling.filmstrip.label')"
  >
    <button
      v-for="asset in assets"
      :key="asset.id"
      type="button"
      role="option"
      class="relative h-16 w-16 shrink-0 overflow-hidden rounded"
      :class="asset.id === currentId ? 'ring-2 ring-accent' : 'opacity-70'"
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
      <span
        v-if="flagsFor(asset.id).pick === 'pick'"
        class="absolute bottom-0 right-0 bg-accent px-1 text-[10px] text-white"
      >{{ t('culling.badges.pick') }}</span>
      <span
        v-if="flagsFor(asset.id).pick === 'reject'"
        class="absolute bottom-0 right-0 bg-danger px-1 text-[10px] text-white"
      >{{ t('culling.badges.reject') }}</span>
      <span
        v-if="flagsFor(asset.id).rating"
        class="absolute left-0 top-0 bg-black/70 px-1 text-[10px] text-accent"
      >{{ flagsFor(asset.id).rating }}★</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const props = defineProps<{ rating: number | null; readonly?: boolean }>()
const emit = defineEmits<{ rate: [value: number] }>()
const { t } = useI18n()

function starClass(n: number): string {
  return (props.rating ?? 0) >= n ? 'text-accent' : 'text-content-muted'
}
</script>

<template>
  <div
    class="flex items-center gap-0.5"
    role="group"
    :aria-label="t('culling.rating.group')"
  >
    <button
      v-for="n in 5"
      :key="n"
      type="button"
      class="text-lg leading-none disabled:cursor-default"
      :class="starClass(n)"
      :disabled="readonly"
      :aria-label="t('culling.rating.star', { n })"
      :aria-pressed="(rating ?? 0) >= n"
      @click="emit('rate', n)"
    >
      ★
    </button>
  </div>
</template>

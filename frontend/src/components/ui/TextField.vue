<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ label: string; type?: string; hint?: string; autocomplete?: string; required?: boolean }>()
const model = defineModel<string>({ required: true })
const id = useId()
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label
      :for="id"
      class="text-sm font-medium text-content"
    >{{ label }}</label>
    <input
      :id="id"
      v-model="model"
      :type="type ?? 'text'"
      :autocomplete="autocomplete"
      :required="required"
      :aria-describedby="hint ? `${id}-hint` : undefined"
      class="rounded-lg border border-border bg-surface-elevated px-3 py-2.5
             text-content focus-visible:outline-2 focus-visible:outline-accent"
    >
    <p
      v-if="hint"
      :id="`${id}-hint`"
      class="text-xs text-content-muted"
    >
      {{ hint }}
    </p>
  </div>
</template>

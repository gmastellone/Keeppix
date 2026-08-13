<script setup lang="ts">
defineProps<{ type?: 'button' | 'submit'; disabled?: boolean; loading?: boolean }>()
</script>

<template>
  <button
    :type="type ?? 'button'"
    :disabled="disabled || loading"
    class="w-full rounded-lg bg-accent px-4 py-2.5 font-medium text-white
           transition-opacity hover:opacity-90 focus-visible:outline-2
           focus-visible:outline-offset-2 focus-visible:outline-accent
           disabled:opacity-50"
  >
    <slot />
  </button>
</template>

<script setup lang="ts">
defineProps<{ message: string }>()
</script>

<template>
  <p
    role="alert"
    class="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger"
  >
    {{ message }}
  </p>
</template>

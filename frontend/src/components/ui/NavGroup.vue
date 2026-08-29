<script setup lang="ts">
// The sidebar's collapsible navigation group (prototype: "Maintenance"
// — keeppix-mockup.html lines 134-146, 2485-2536). It opens on its own
// when the current view is one of its sub-items (`active`) and **cannot
// be closed by clicking the toggle while that's true**: the prototype
// computes this as `maintOpen = state.navMaintOpen || maintActive` — the
// OR can't be canceled by a click, which only toggles its own manual
// state (`navMaintOpen`), never `maintActive`.
import { computed, ref } from 'vue'

const props = defineProps<{ label: string; active: boolean }>()

const manuallyOpen = ref(false)
const open = computed(() => manuallyOpen.value || props.active)

function toggle() {
  manuallyOpen.value = !manuallyOpen.value
}
</script>

<template>
  <div>
    <button
      type="button"
      :aria-expanded="open"
      class="flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-sm
             text-content hover:bg-border/30"
      :class="active && 'font-semibold'"
      @click="toggle"
    >
      <span class="flex items-center gap-2.5">
        <slot name="icon" />
        <span>{{ label }}</span>
      </span>
      <svg
        viewBox="0 0 20 20"
        class="h-3.5 w-3.5 opacity-60 transition-transform"
        :class="[open && 'rotate-180', active && 'opacity-100']"
        :style="{ transitionDuration: 'var(--duration-arrow)' }"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="M6 8l4 4 4-4"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
    <div
      v-if="open"
      class="pl-6"
    >
      <slot />
    </div>
  </div>
</template>

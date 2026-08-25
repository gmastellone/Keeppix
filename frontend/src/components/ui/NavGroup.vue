<script setup lang="ts">
// SP-25: il gruppo di navigazione a scomparsa della barra laterale
// (prototipo, "Manutenzione" — keeppix-mockup.html righe 134-146,
// 2485-2536). Si apre da solo quando la vista corrente è una delle sue
// sotto-voci (`active`) e **non si chiude cliccando l'interruttore mentre
// è così**: il prototipo lo calcola con `maintOpen = state.navMaintOpen ||
// maintActive` — l'OR non è annullabile dal clic, che alterna solo il
// proprio stato manuale (`navMaintOpen`), mai `maintActive`.
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

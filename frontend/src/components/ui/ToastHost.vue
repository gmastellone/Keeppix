<script setup lang="ts">
// The panel that hosts toasts from `useToastStore` — a single
// `<ToastHost />` mounted once in `App.vue`, never one per screen. The
// timing logic (appearance delay, per-kind lifetime, pause on hover)
// lives in the store, not here: this component just translates state
// into markup and intercepts hover/click.
import { useToastStore } from '@/stores/toast'

const store = useToastStore()
</script>

<template>
  <div
    class="pointer-events-none fixed bottom-5 left-1/2 z-90 flex -translate-x-1/2 flex-col items-center gap-2"
  >
    <div
      v-for="toast in store.toasts"
      :key="toast.id"
      :role="toast.kind === 'ok' ? undefined : 'alert'"
      class="pointer-events-auto flex items-center gap-2.5 rounded-lg bg-[var(--color-content)]
             px-4 py-2.5 text-[12.5px] whitespace-nowrap text-[var(--color-surface)] shadow-lg
             transition-[opacity,transform]"
      :class="[
        toast.visible ? 'translate-y-0 opacity-100' : 'translate-y-2.5 opacity-0',
        toast.kind === 'error' && 'border-l-3 border-[var(--color-toast-danger)]',
        toast.kind === 'partial' && 'border-l-3 border-[var(--color-toast-warn)]'
      ]"
      :style="{ transitionDuration: 'var(--duration-base)', transitionTimingFunction: 'var(--easing-standard)' }"
      @mouseenter="toast.action && store.pause(toast.id)"
      @mouseleave="toast.action && store.resume(toast.id)"
    >
      <span>{{ toast.message }}</span>
      <span
        v-if="toast.action"
        role="button"
        tabindex="0"
        class="cursor-pointer py-0.5 font-bold whitespace-nowrap underline
               focus-visible:rounded focus-visible:outline-2 focus-visible:outline-offset-2
               focus-visible:outline-accent"
        @click="store.runAction(toast.id)"
        @keydown.enter.prevent="store.runAction(toast.id)"
        @keydown.space.prevent="store.runAction(toast.id)"
      >
        {{ toast.action.label }}
      </span>
    </div>
  </div>
</template>

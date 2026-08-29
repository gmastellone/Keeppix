<script setup lang="ts">
// Shared popover built on reka-ui — one of the two building blocks the
// rest of the app's dialogs/menus/popovers are built on (the other is
// `Dialog.vue`). Covers the various popup menus (account menu, lightbox
// "more actions", batch quick-selector, face-tile menu, map popover,
// album-creation picklist) plus the person/tag picker when a full-screen
// modal dialog isn't needed.
//
// The prototype here only closed halfway ("click outside closes, Esc
// only closes halfway"). reka-ui's `PopoverContent` (on top of
// `DismissableLayer`) closes on both on its own — no handler written
// here — **and** when two popovers are nested, only the one highest in
// the stack reacts to Esc, because each `DismissableLayer` registers
// itself in its own layer stack: "layered Esc" is library behavior, not
// something to orchestrate by hand.
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'

const open = defineModel<boolean>('open')

// `escDismisses`: the batch quick-selector is the one consumer of this
// component that deviates from the general rule above ("click outside
// closes, Esc only closes halfway") — its spec says "Esc does not close
// this panel", since there's no dedicated handling in the culling flow's
// global keyboard handler. Rather than a new component, this is a single
// optional prop, defaulting to `true` so the other existing consumers
// are unaffected.
const props = withDefaults(
  defineProps<{
    side?: 'top' | 'right' | 'bottom' | 'left'
    align?: 'start' | 'center' | 'end'
    sideOffset?: number
    escDismisses?: boolean
  }>(),
  { side: 'bottom', align: 'start', sideOffset: 6, escDismisses: true }
)

function onEscapeKeyDown(event: Event) {
  if (!props.escDismisses) event.preventDefault()
}
</script>

<template>
  <PopoverRoot v-model:open="open">
    <PopoverTrigger as-child>
      <slot name="trigger" />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :side="side"
        :align="align"
        :side-offset="sideOffset"
        class="z-50 min-w-40 rounded-lg border border-[var(--color-border)]
               bg-[var(--color-surface-elevated)] p-1 shadow-lg
               focus:outline-none"
        @escape-key-down="onEscapeKeyDown"
      >
        <slot />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>

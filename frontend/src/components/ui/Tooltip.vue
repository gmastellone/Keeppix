<script setup lang="ts">
// Generic tooltip for icon-only buttons (`[data-tip]` in the prototype,
// keeppix-mockup.html lines 382-395). Decorative, not the source of
// accessibility — that remains the `aria-label` on the real control
// inside the slot (per the prototype's own comment, line 1128: "meaning
// is carried by the tooltip (desktop) + aria-label (always)"). That's
// why the bubble is `aria-hidden`: a screen reader must not read it
// twice.
defineProps<{ label: string }>()
</script>

<template>
  <span class="group/tooltip relative inline-flex">
    <slot />
    <span
      aria-hidden="true"
      class="tooltip-bubble pointer-events-none absolute bottom-[calc(100%+8px)] left-1/2 z-30
             -translate-x-1/2 translate-y-[3px] rounded-md bg-[var(--color-content)] px-2 py-1
             text-[11px] font-semibold whitespace-nowrap text-[var(--color-surface)] opacity-0
             transition-[opacity,transform] group-hover/tooltip:translate-y-0
             group-hover/tooltip:opacity-100 group-focus-within/tooltip:translate-y-0
             group-focus-within/tooltip:opacity-100"
      :style="{ transitionDuration: 'var(--duration-fast)', transitionTimingFunction: 'var(--easing-standard)' }"
    >
      {{ label }}
    </span>
  </span>
</template>

<style scoped>
/* The prototype disabled this on a hand-computed `device-mobile` class;
   here we use the standard equivalent — no touch-only device supports a
   real hover, which is exactly the criterion the prototype's comment
   described ("no hover on touch, the icon has to stand on its own"). */
@media not all and (hover: hover) and (pointer: fine) {
  .tooltip-bubble {
    display: none;
  }
}
</style>

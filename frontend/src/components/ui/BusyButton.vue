<script setup lang="ts">
// The busy button (prototype: `.btn.is-busy`/`setBtnBusy`, lines 864-867
// and 2638-2657 of keeppix-mockup.html). Does two things at once — says
// "I'm working" and prevents double-submission, which on a bulk action
// is the easiest way to accidentally duplicate an operation. Distinct
// from `Button.vue` (the full-width primary CTA used in setup flows):
// this is the generic `.btn` — variant, often icon-only — used by the
// selection bar and bulk actions.
withDefaults(
  defineProps<{
    variant?: 'default' | 'primary' | 'danger' | 'ghost'
    busy?: boolean
    iconOnly?: boolean
    type?: 'button' | 'submit'
  }>(),
  { variant: 'default', busy: false, iconOnly: false, type: 'button' }
)
</script>

<template>
  <button
    :type="type"
    :disabled="busy"
    :aria-busy="busy || undefined"
    class="inline-flex items-center rounded-lg border px-3.5 py-2 text-[13px] font-semibold
           transition-colors"
    :class="[
      variant === 'primary' && 'border-accent bg-accent text-accent-text hover:brightness-105',
      variant === 'danger' && 'border-danger bg-transparent text-danger hover:bg-danger/10',
      variant === 'ghost' && 'border-transparent bg-transparent text-content hover:bg-border/40',
      variant === 'default' && 'border-border bg-surface-elevated text-content hover:bg-border/40',
      busy && 'pointer-events-none opacity-75',
      iconOnly ? 'h-8 w-8 justify-center gap-0 p-0' : 'gap-1.5'
    ]"
  >
    <span
      v-if="busy"
      aria-hidden="true"
      class="spinner"
      :class="[iconOnly ? '' : 'spinner-sm', variant === 'primary' && 'spinner-current']"
    />
    <!-- Icon-only and busy: the spinner replaces the icon (the prototype
         never puts two indicators side by side in the space of one
         glyph). Otherwise the label stays, and the spinner sits next to
         it. -->
    <slot v-if="!(busy && iconOnly)" />
  </button>
</template>

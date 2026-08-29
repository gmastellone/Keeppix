<script setup lang="ts">
// The initials avatar (prototype: `.avatar`, lines 220-229 of
// keeppix-mockup.html). A deliberate choice, not a contrast oversight:
// the text stays **always white**, never `--color-accent-text` (dark in
// light theme), because the initials need to stay legible on top of any
// background: the default orange, a hash-based color assigned to another
// person in a share, or a color the user picked in Profile. Initials are
// computed with the same algorithm as the prototype (line 4373): one
// character per word of the name, at most two, uppercase.
//
// The color isn't decided here: it's a prop. The component guarantees
// that the same (name, color) pair always renders identically —
// "synchronized everywhere" is the responsibility of whoever reads the
// color from a single source (the current user's preferences, or the
// person's hash) and passes it in here, not something a pure-rendering
// component can guarantee on its own.
withDefaults(defineProps<{ name: string; color?: string | null; size?: 'sm' | 'lg' }>(), {
  color: null,
  size: 'sm'
})

// The only two sizes actually used in the prototype: 28px/12px from the
// user footer and sidebar (base `.avatar` CSS) and 56px/20px from the
// large avatar in Profile (line 7187) — not an invented ratio between
// the two, which isn't linear in the prototype.
const DIMENSIONS = { sm: { box: 28, font: 12 }, lg: { box: 56, font: 20 } } as const

function initials(name: string): string {
  return name
    .split(' ')
    .filter(Boolean)
    .map((word) => word[0])
    .join('')
    .slice(0, 2)
    .toUpperCase()
}
</script>

<template>
  <span
    class="inline-flex flex-none items-center justify-center rounded-full font-bold text-white"
    :style="{
      width: `${DIMENSIONS[size].box}px`,
      height: `${DIMENSIONS[size].box}px`,
      fontSize: `${DIMENSIONS[size].font}px`,
      background: color ?? 'var(--color-accent)'
    }"
    :aria-label="name"
  >
    {{ initials(name) }}
  </span>
</template>

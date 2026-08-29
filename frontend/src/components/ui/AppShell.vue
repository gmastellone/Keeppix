<script setup lang="ts">
// Scope here: **only** the switching mechanism between the desktop and
// mobile scaffolding. The tab bar that routes on `state.view`, per-view
// titles, the culling badge, and the account menu (see the "Mobile
// shell" spec) stay out of scope: they depend on the router, which does
// not exist yet. Wiring them up now would mean inventing routing
// conventions that the router could later contradict. This component
// only exposes the slots that the real shell will populate once the
// router exists.
//
// Binding design note: **"switch by width, not by toggle."** The
// prototype used `state.device`, a manual toggle for demos —
// `#app.device-mobile` was a static class, never tied to actual viewport
// width. Here we use a real media query instead. **No numeric threshold
// is specified** anywhere in the spec or mockup (verified: "below a
// certain width", never a figure) — 768px is Tailwind's `md` breakpoint,
// already the project's standard, not a value measured from the
// prototype. This is a declared assumption, not a silent one.
import { useIsMobile } from '@/composables/useIsMobile'

const { isMobile } = useIsMobile()

defineExpose({ isMobile })
</script>

<template>
  <div class="flex h-full">
    <template v-if="!isMobile">
      <slot name="sidebar" />
      <!-- `relative`: positioning anchor for the upload subsystem's drop
           veil (`UploadDropVeil.vue`, `position: absolute; inset:0`) —
           covers topbar+content, not the sidebar, same area as
           `#dropOverlayHost` in the mockup (`.main`, lines 1433-1446 of
           keeppix-mockup.html). -->
      <div class="relative flex min-w-0 flex-1 flex-col">
        <slot name="topbar" />
        <slot />
      </div>
    </template>
    <template v-else>
      <div class="flex h-full w-full flex-col overflow-hidden">
        <slot name="mobile-header" />
        <div class="min-h-0 flex-1 overflow-y-auto">
          <slot />
        </div>
        <slot name="mobile-tabbar" />
      </div>
    </template>
  </div>
</template>

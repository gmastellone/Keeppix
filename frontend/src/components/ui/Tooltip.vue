<script setup lang="ts">
// SP-7 (Fase 11 Task 2): il tooltip generico dei pulsanti icon-only
// (`[data-tip]` nel prototipo, keeppix-mockup.html righe 382-395). Decorativo,
// non la fonte dell'accessibilità — quella resta l'`aria-label` sul controllo
// reale dentro lo slot (commento del prototipo stesso, riga 1128:
// "il significato lo porta il tooltip (desktop) + aria-label (sempre)").
// Per questo la bolla è `aria-hidden`: uno screen reader non deve leggerla
// due volte.
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
/* Il prototipo lo disattiva su una classe `device-mobile` calcolata a mano;
   qui usiamo l'equivalente standard — nessun dispositivo touch-only supporta
   un hover reale, è esattamente il criterio che il commento del prototipo
   descrive ("niente hover sul touch, l'icona deve bastare da sola"). */
@media not all and (hover: hover) and (pointer: fine) {
  .tooltip-bubble {
    display: none;
  }
}
</style>

<script setup lang="ts">
// SP-14 (Fase 11 Task 2): popover condiviso su reka-ui — la seconda delle
// due fondamenta su cui il piano chiede di costruire i ventiquattro dialog/
// menu/popover del documento (l'altra è `Dialog.vue`). Copre i sei menu a
// comparsa (account, "altre azioni" del lightbox, selettore rapido di
// lotto, menu sul riquadro del volto, popover della mappa, picklist di
// creazione album) più il selettore di persona/tag quando non serve un
// dialog modale a schermo intero.
//
// Il prototipo qui chiude solo a metà (documento funzionale, SP-14: "click
// fuori chiude, Esc chiude solo a metà"). `PopoverContent` di reka-ui
// (sopra `DismissableLayer`) chiude su entrambi per conto proprio — nessun
// gestore scritto qui — **e** quando due popover sono annidati solo quello
// più in alto nello stack reagisce a Esc, perché ogni `DismissableLayer` si
// registra nel proprio stack di livelli: "Esc a livelli" è comportamento
// della libreria, non qualcosa da orchestrare a mano.
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'

const open = defineModel<boolean>('open')

withDefaults(
  defineProps<{
    side?: 'top' | 'right' | 'bottom' | 'left'
    align?: 'start' | 'center' | 'end'
    sideOffset?: number
  }>(),
  { side: 'bottom', align: 'start', sideOffset: 6 }
)
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
      >
        <slot />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>

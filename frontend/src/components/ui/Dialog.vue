<script setup lang="ts">
// SP-5 (Fase 11 Task 2): dialog condiviso su reka-ui. Il prototipo qui ha
// due difetti dichiarati (documento funzionale §"Attriti minori", 3.6): il
// focus non è confinato e il click sul velo non chiude. Entrambi risolti
// gratis usando `DialogContent`/`DialogOverlay` reali — reka-ui intrappola
// il focus (variante modale, sempre attiva su `DialogContent`) e chiude su
// click esterno/Esc per conto proprio (`DismissableLayer`), non codice
// scritto qui.
import { DialogClose, DialogContent, DialogDescription, DialogOverlay, DialogPortal, DialogRoot, DialogTitle } from 'reka-ui'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  title: string
  description?: string
  /**
   * L'elemento su cui portare il focus all'apertura, al posto del primo
   * elemento tabbable predefinito. Le due eccezioni deliberate che il piano
   * chiede di **preservare**, non uniformare: nel dialog di eliminazione va
   * sull'opzione meno distruttiva, in quello di conferma su "Annulla" — chi
   * preme Invio d'istinto compie l'azione innocua.
   */
  initialFocus?: HTMLElement | null
}>()

function onOpenAutoFocus(event: Event) {
  if (props.initialFocus) {
    event.preventDefault()
    props.initialFocus.focus()
  }
}
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay
        class="fixed inset-0 z-40 bg-black/50 transition-opacity"
        :style="{ transitionDuration: 'var(--duration-base)' }"
      />
      <DialogContent
        class="fixed top-1/2 left-1/2 z-50 w-[calc(100%-2rem)] max-w-md -translate-x-1/2 -translate-y-1/2
               rounded-xl bg-[var(--color-surface-elevated)] p-6 shadow-xl
               focus:outline-none"
        @open-auto-focus="onOpenAutoFocus"
      >
        <DialogTitle class="pr-8 text-lg font-semibold text-[var(--color-content)]">
          {{ title }}
        </DialogTitle>
        <DialogDescription
          v-if="description"
          class="mt-1 text-sm text-[var(--color-content-muted)]"
        >
          {{ description }}
        </DialogDescription>
        <div class="mt-4">
          <slot />
        </div>
        <DialogClose
          :aria-label="t('ui.dialog.close')"
          class="absolute top-4 right-4 rounded-md p-1 text-[var(--color-content-muted)]
                 transition-colors hover:text-[var(--color-content)]
                 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          :style="{ transitionDuration: 'var(--duration-fast)' }"
        >
          <svg
            viewBox="0 0 20 20"
            class="h-5 w-5"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M5 5l10 10M15 5L5 15"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
        </DialogClose>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

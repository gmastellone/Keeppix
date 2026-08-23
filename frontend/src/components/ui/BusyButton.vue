<script setup lang="ts">
// SP-30: il pulsante occupato (prototipo, `.btn.is-busy`/`setBtnBusy`,
// righe 864-867 e 2638-2657 di keeppix-mockup.html). Serve a due cose
// insieme — dire "sto lavorando" e impedire il doppio invio, che su
// un'azione di massa è il modo più facile per duplicare un'operazione.
// Distinto da `Button.vue` (la CTA primaria a piena larghezza dei flussi
// di impostazione): questo è il `.btn` generico — variante, spesso
// icon-only — usato dalla barra di selezione e dalle azioni di massa.
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
    <!-- Icon-only e occupato: lo spinner sostituisce l'icona (il prototipo
         non affianca due indicatori nello spazio di un solo glifo).
         Altrimenti l'etichetta resta, lo spinner le si affianca. -->
    <slot v-if="!(busy && iconOnly)" />
  </button>
</template>

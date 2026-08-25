<script setup lang="ts">
// SP-24: il controllo a segmenti (prototipo, `.seg-control`/`.seg-option`
// — es. righe 4441-4455 di keeppix-mockup.html — e `wireSegGroup`, riga
// 4519). Un radiogroup di opzioni mutuamente esclusive. Il prototipo
// gestisce solo clic/Invio/Spazio via `bindActivatable`: le frecce **non
// ci sono**, nota vincolante esplicita del piano ("roving tabindex e
// frecce, il prototipo non le ha") — qui sono un'aggiunta reale, non
// solo trascritta. "Nei filtri della modifica in blocco include sempre
// 'Non modificare'" è una regola per chi *chiama* questo componente
// (l'opzione va nell'array `options` passato), non qualcosa che un
// controllo generico può imporre da solo.
import { ref } from 'vue'

export interface SegmentedOption {
  value: string
  label: string
}

const props = defineProps<{ options: SegmentedOption[]; ariaLabel?: string }>()
const model = defineModel<string>({ required: true })

const optionButtons = ref<HTMLElement[]>([])

function selectIndex(index: number, focus: boolean) {
  const option = props.options[index]
  if (!option) return
  model.value = option.value
  if (focus) optionButtons.value[index]?.focus()
}

function onKeydown(event: KeyboardEvent, index: number) {
  if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
    event.preventDefault()
    selectIndex((index + 1) % props.options.length, true)
  } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
    event.preventDefault()
    selectIndex((index - 1 + props.options.length) % props.options.length, true)
  }
}
</script>

<template>
  <div
    role="radiogroup"
    :aria-label="ariaLabel"
    class="inline-flex w-fit gap-0.5 rounded-[9px] bg-border/40 p-[3px]"
  >
    <button
      v-for="(option, index) in options"
      :key="option.value"
      ref="optionButtons"
      type="button"
      role="radio"
      :aria-checked="option.value === model"
      :tabindex="option.value === model ? 0 : -1"
      class="rounded-md px-3 py-1.5 text-[12.5px] font-medium text-content-muted transition-colors"
      :class="option.value === model && 'bg-surface-elevated text-content shadow-sm'"
      :style="{ transitionDuration: 'var(--duration-base)' }"
      @click="selectIndex(index, false)"
      @keydown="onKeydown($event, index)"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<script setup lang="ts">
// The segmented control (prototype: `.seg-control`/`.seg-option` — e.g.
// lines 4441-4455 of keeppix-mockup.html — and `wireSegGroup`, line
// 4519). A radiogroup of mutually exclusive options. The prototype only
// handles click/Enter/Space via `bindActivatable`: arrow keys **are not
// present there** — here they're a real addition, not just transcribed.
// "Bulk-edit filters should always include 'Don't change'" is a rule for
// whoever *calls* this component (that option goes into the `options`
// array passed in), not something a generic control can enforce on its
// own.
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

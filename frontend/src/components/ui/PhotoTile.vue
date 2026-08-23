<script setup lang="ts">
// SP-1 (documento funzionale §10, definizione canonica) + SP-15 (badge
// del formato): il mattone di ogni vista a griglia (Foto, Preferiti,
// Album, dettaglio Persona, risultati di Cerca). Tre stop di
// tabulazione, in quest'ordine: apri → cerchietto di selezione →
// cuoricino — l'ordine del DOM qui sotto è lo stesso ordine, non serve
// alcun `tabindex` esplicito diverso da 0.
//
// "Nient'altro" (§10.2): niente nome file, data, stelle, pick/scarta,
// "in album" sulla tessera — quell'informazione vive solo nell'etichetta
// accessibile e nel lightbox. Il componente non inventa markup visivo
// in più di quello che il documento elenca.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { LONG_PRESS_THRESHOLD_MS, LONG_PRESS_VIBRATE_MS } from '@/design/tokens'

export type StackType = 'raw_jpeg' | 'raw_only' | 'jpeg'

const props = withDefaults(
  defineProps<{
    thumbnailUrl: string
    filename: string
    /** Già formattata dal chiamante (es. "12 luglio 2026") — non
     * l'anno fisso "2026" del prototipo, che è una costante di demo, non
     * un formato da riprodurre su dati reali. */
    dateLabel: string
    isFavorite: boolean
    stackType: StackType
    selected: boolean
    selectionMode: boolean
    /** Il tocco prolungato (§10.4, 500ms + vibrazione) è gestito qui,
     * ma solo se il chiamante lo attiva: sapere se siamo su mobile è
     * compito di `AppShell` (già costruito), non di questo componente
     * che non deve reimplementare `matchMedia`. */
    enableLongPress?: boolean
  }>(),
  { enableLongPress: false }
)

const emit = defineEmits<{ open: []; 'toggle-select': []; 'toggle-favorite': [] }>()

const { t } = useI18n()

// SP-15: "RAW" solo negativo, "RAW+JPEG" negativo e JPEG affiancati
// come un unico scatto, nessun badge per il solo JPEG — stessa logica
// di `rawBadgeLabel` nel prototipo (riga 4095), non un'invenzione.
const rawBadgeLabel = computed(() => {
  if (props.stackType === 'raw_jpeg') return 'RAW+JPEG'
  if (props.stackType === 'raw_only') return 'RAW'
  return ''
})

const accessibleLabel = computed(
  () => `${props.filename}, ${props.dateLabel}${props.isFavorite ? t('ui.photoTile.favoriteSuffix') : ''}`
)

function activate() {
  if (props.selectionMode) emit('toggle-select')
  else emit('open')
}

let longPressTimer: ReturnType<typeof setTimeout> | null = null
const suppressNextClick = ref(false)

function startLongPress() {
  if (!props.enableLongPress) return
  longPressTimer = setTimeout(() => {
    longPressTimer = null
    suppressNextClick.value = true
    if (navigator.vibrate) navigator.vibrate(LONG_PRESS_VIBRATE_MS)
    emit('toggle-select')
  }, LONG_PRESS_THRESHOLD_MS)
}

function cancelLongPress() {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
}

function onOpenClick() {
  if (suppressNextClick.value) {
    suppressNextClick.value = false
    return
  }
  activate()
}
</script>

<template>
  <div
    class="group relative overflow-hidden rounded-[7px] border border-border"
    :class="selected && 'outline-[2.5px] -outline-offset-[2.5px] outline-accent'"
  >
    <img
      :src="thumbnailUrl"
      alt=""
      class="pointer-events-none block h-full w-full object-cover"
    >
    <button
      type="button"
      :aria-label="t('ui.photoTile.open', { label: accessibleLabel })"
      class="absolute inset-0 z-[1] cursor-pointer"
      @click="onOpenClick"
      @pointerdown="startLongPress"
      @pointerup="cancelLongPress"
      @pointerleave="cancelLongPress"
      @pointercancel="cancelLongPress"
      @contextmenu="enableLongPress && $event.preventDefault()"
    />
    <button
      type="button"
      role="checkbox"
      :aria-checked="selected"
      :aria-label="t('ui.photoTile.select', { label: accessibleLabel })"
      class="absolute top-1.5 left-1.5 z-[2] flex h-[22px] w-[22px] items-center justify-center
             rounded-full border-2 border-white/90 bg-black/35 text-white opacity-0
             transition-[opacity,background-color,border-color] duration-[var(--duration-fast)]
             ease-[var(--easing-standard)] group-hover:opacity-100 group-focus-within:opacity-100
             focus-visible:opacity-100"
      :class="[
        selected && 'border-white bg-accent text-accent-text opacity-100',
        selectionMode && 'opacity-100'
      ]"
      @click.stop="emit('toggle-select')"
    >
      <svg
        v-if="selected"
        viewBox="0 0 24 24"
        width="12"
        height="12"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M20 6L9 17l-5-5" />
      </svg>
    </button>
    <div
      v-if="stackType !== 'jpeg'"
      aria-hidden="true"
      class="pointer-events-none absolute top-1.5 left-1.5 z-0 rounded-[4px] bg-black/55 px-1.5 py-0.5
             text-[9.5px] font-bold tracking-wide text-white opacity-100 transition-opacity
             duration-[var(--duration-fast)] group-hover:opacity-0 group-focus-within:opacity-0"
      :class="selected && 'opacity-0'"
    >
      {{ rawBadgeLabel }}
    </div>
    <div
      v-if="!selectionMode"
      class="absolute top-1.5 right-1.5 z-[2] flex gap-1"
    >
      <button
        type="button"
        :aria-label="isFavorite ? t('ui.photoTile.removeFavorite') : t('ui.photoTile.addFavorite')"
        class="flex h-[22px] w-[22px] items-center justify-center text-white opacity-0
               transition-opacity duration-[var(--duration-fast)] ease-[var(--easing-standard)]
               [filter:drop-shadow(0_1px_2px_rgba(0,0,0,.65))_drop-shadow(0_0_3px_rgba(0,0,0,.45))]
               group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
        :class="isFavorite && 'opacity-100'"
        @click.stop="emit('toggle-favorite')"
      >
        <svg
          viewBox="0 0 24 24"
          width="19"
          height="19"
          :fill="isFavorite ? 'currentColor' : 'none'"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path
            d="M12 21s-7.5-4.6-10-9C.3 8.3 2 4 6 4c2.2 0 3.7 1.2 6 3.6C14.3 5.2 15.8 4 18 4c4 0 5.7 4.3 4 8-2.5 4.4-10 9-10 9z"
          />
        </svg>
      </button>
    </div>
  </div>
</template>

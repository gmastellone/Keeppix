<script setup lang="ts">
// Fase 11 Task 15 (1/N), §53 "Dialog 'modifica tag'" — documento
// funzionale verificato riga per riga (righe 7925-8058).
//
// **Soglia — conversione percentuale ↔ frazione**: lo slider del
// documento è 30-95 (%), ma `Tag.threshold` sul backend è una frazione
// 0-1 (colonna `real DEFAULT 0.75`, confrontata direttamente col punteggio
// di coseno in `AssetTagRepo`) — mai una percentuale. Convertita qui, non
// sul backend: `threshold * 100` in lettura, `/100` in scrittura.
//
// **Duplicati NON permessi qui, a differenza del mockup**: `TagRepo`
// applica `UNIQUE(name, kind)` per davvero (409 Conflict) — "i nomi
// duplicati sono permessi" del documento vale solo nel prototipo, senza
// un vero indice unico. Il 409 diventa un errore reale sotto il campo
// nome, non solo il caso "campo vuoto" che il documento descrive.
//
// **Colore — stringa CSS opaca, non una tinta HSL pura**: il backend
// accetta qualunque testo (`color: Option<String>`, nessuna validazione),
// consumato da `TagPickerDialog.vue` già come `background` diretto — le
// pastiglie qui scrivono `hsl(H,60%,50%)` per intero, riprendendo la
// stessa tavolozza di 10 tinte del documento (`TAG_SWATCH_HUES`) ma come
// colore completo, coerente con l'uso reale già in produzione.
//
// **Pastiglie in un vero `radiogroup` con anello di focus visibile**: il
// documento segnala esplicitamente l'assenza di entrambi come "un difetto
// di accessibilità reale" nel prototipo — corretto qui, non riprodotto,
// stesso principio già seguito per le frecce di `SegmentedControl.vue`
// (Task 14 1/N).
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { createTag, deleteTag, patchTag, type Tag } from '@/api/tags'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'

const TAG_SWATCH_HUES = [24, 150, 205, 340, 270, 34, 195, 0, 120, 290]
const DEFAULT_THRESHOLD_PERCENT = 75

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ tag: Tag | null; categories: Tag[]; tagCount: number }>()
const emit = defineEmits<{ saved: []; deleted: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const isEdit = computed(() => props.tag !== null)

const name = ref('')
const prompt = ref('')
const categoryId = ref('')
const color = ref<string>('')
const thresholdPercent = ref(DEFAULT_THRESHOLD_PERCENT)
const nameError = ref('')
const saving = ref(false)
const deleteConfirmOpen = ref(false)

function swatchColor(hue: number): string {
  return `hsl(${hue}, 60%, 50%)`
}

function reset() {
  if (props.tag) {
    name.value = props.tag.name
    prompt.value = props.tag.prompt ?? ''
    categoryId.value = props.tag.parent_id ?? ''
    color.value = props.tag.color ?? ''
    thresholdPercent.value = Math.round((props.tag.threshold ?? 0.75) * 100)
  } else {
    name.value = ''
    prompt.value = ''
    categoryId.value = ''
    color.value = swatchColor(TAG_SWATCH_HUES[props.tagCount % TAG_SWATCH_HUES.length])
    thresholdPercent.value = DEFAULT_THRESHOLD_PERCENT
  }
  nameError.value = ''
}

// `immediate: true`: il dialog può nascere già aperto (stesso bug reale
// già trovato in `ProblemFilesDialog.vue`, Task 13 3/N) — senza, un
// editor aperto direttamente in modifica parte con i campi vuoti invece
// che precompilati dal tag.
watch(
  open,
  (isOpen) => {
    if (isOpen) reset()
  },
  { immediate: true }
)

async function save() {
  const trimmedName = name.value.trim()
  if (!trimmedName) {
    nameError.value = t('tagEditor.nameRequired')
    return
  }
  saving.value = true
  nameError.value = ''
  try {
    const payload = {
      name: trimmedName,
      parent_id: categoryId.value || null,
      prompt: prompt.value.trim() || null,
      color: color.value || null,
      threshold: thresholdPercent.value / 100
    }
    if (props.tag) {
      await patchTag(props.tag.id, payload)
      toast.show(t('tagEditor.savedToast', { name: trimmedName }))
    } else {
      await createTag({ ...payload, kind: 'tag' })
      toast.show(t('tagEditor.createdToast', { name: trimmedName }))
    }
    open.value = false
    emit('saved')
  } catch (err) {
    nameError.value = t('tagEditor.nameConflict')
    void err
  } finally {
    saving.value = false
  }
}

// §53.3 punto 8: "Elimina tag" chiude *prima* questo dialog, poi apre lo
// stesso dialog di conferma della pagina — mai i due sovrapposti.
function askDelete() {
  open.value = false
  deleteConfirmOpen.value = true
}

async function confirmDelete() {
  if (!props.tag) return
  try {
    await deleteTag(props.tag.id)
    toast.show(t('tagEditor.deletedToast', { name: props.tag.name }))
    open.value = false
    emit('deleted')
  } catch {
    toast.showError(t('tagEditor.deleteError'))
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="isEdit ? t('tagEditor.editTitle', { name: tag?.name }) : t('tagEditor.newTitle')"
  >
    <div class="flex flex-col gap-3.5">
      <TextField
        v-model="name"
        :label="t('tagEditor.name')"
      />
      <p
        v-if="nameError"
        class="-mt-2 text-xs text-danger"
      >
        {{ nameError }}
      </p>
      <TextField
        v-model="prompt"
        :label="`${t('tagEditor.prompt')} (${t('tagEditor.optional')})`"
      />
      <p class="-mt-2 text-[11.5px] leading-normal text-content-muted">
        {{ t('tagEditor.hint') }}
      </p>
      <label class="flex flex-col gap-1.5 text-sm font-medium text-content">
        {{ t('tagEditor.category') }}
        <select
          v-model="categoryId"
          class="rounded-lg border border-border bg-surface-elevated px-3 py-2.5 text-sm"
        >
          <option value="">
            {{ t('tagEditor.noCategory') }}
          </option>
          <option
            v-for="cat in categories"
            :key="cat.id"
            :value="cat.id"
          >
            {{ cat.name }}
          </option>
        </select>
      </label>
      <div>
        <p class="mb-1.5 text-sm font-medium text-content">
          {{ t('tagEditor.color') }}
        </p>
        <div
          role="radiogroup"
          :aria-label="t('tagEditor.color')"
          class="flex flex-wrap gap-2"
        >
          <button
            v-for="hue in TAG_SWATCH_HUES"
            :key="hue"
            type="button"
            role="radio"
            :aria-checked="color === swatchColor(hue)"
            :aria-label="t('tagEditor.color')"
            class="h-[26px] w-[26px] shrink-0 rounded-full border-2 transition-colors
                   focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            :style="{
              background: swatchColor(hue),
              borderColor: color === swatchColor(hue) ? 'var(--color-content)' : 'transparent'
            }"
            @click="color = swatchColor(hue)"
          />
        </div>
      </div>
      <div>
        <label class="text-sm font-medium text-content">
          {{ t('tagEditor.threshold', { n: thresholdPercent }) }}
        </label>
        <input
          v-model.number="thresholdPercent"
          type="range"
          min="30"
          max="95"
          class="mt-1.5 w-full"
        >
        <p class="mt-1.5 text-[11.5px] leading-normal text-content-muted">
          {{ t('tagEditor.thresholdHint', { n: thresholdPercent }) }}
        </p>
      </div>
      <div class="mt-1 flex items-center gap-2">
        <button
          type="button"
          class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text disabled:opacity-60"
          :disabled="saving"
          @click="save"
        >
          {{ isEdit ? t('tagEditor.save') : t('tagEditor.create') }}
        </button>
        <button
          type="button"
          class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
          @click="open = false"
        >
          {{ t('ui.dialog.cancel') }}
        </button>
        <button
          v-if="isEdit"
          type="button"
          class="ml-auto rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10"
          @click="askDelete"
        >
          {{ t('tagEditor.delete') }}
        </button>
      </div>
    </div>
  </Dialog>

  <ConfirmDialog
    v-if="tag"
    v-model:open="deleteConfirmOpen"
    :title="t('tagEditor.deleteConfirmTitle', { name: tag.name })"
    :description="tag.assignment_count > 0
      ? t('tagEditor.deleteConfirmDescriptionWithPhotos', { n: tag.assignment_count })
      : t('tagEditor.deleteConfirmDescriptionEmpty')"
    :confirm-label="t('tagEditor.delete')"
    @confirm="confirmDelete"
  />
</template>

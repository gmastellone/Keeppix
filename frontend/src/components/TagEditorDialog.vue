<script setup lang="ts">
// "Edit tag" dialog.
//
// **Threshold — not a 0-100% confidence, `threshold * 100` directly**: the
// original mockup imagined a 30-95 (%) slider read as confidence, but
// `Tag.threshold` is OpenCLIP XLM-R IT/EN's raw cosine score — 0.10-0.20
// even for correct matches, never close to 1.0 (recalibrated in
// `migrations/0051_tag_threshold_default_openclip.sql`, which also brings
// the default to 0.20). Range 5-40%, not 30-95: covering the real score
// with margin above and below, not the confidence scale the mockup
// assumed before the real score distribution was discovered. Converted
// here, not on the backend: `threshold * 100` on read, `/100` on write.
//
// **Duplicates NOT allowed here, unlike the mockup**: `TagRepo` applies
// `UNIQUE(name, kind)` for real (409 Conflict) — the mockup's "duplicate
// names are allowed" only holds in the prototype, without a real unique
// index. The 409 becomes a real error under the name field, not just the
// "empty field" case the mockup describes.
//
// **Color — an opaque CSS string, not a pure HSL hue**: the backend
// accepts any text (`color: Option<String>`, no validation), already
// consumed by `TagPickerDialog.vue` directly as `background` — the
// swatches here write out `hsl(H,60%,50%)` in full, reusing the mockup's
// same 10-hue palette (`TAG_SWATCH_HUES`) but as a complete color,
// consistent with the real usage already in production.
//
// **Swatches in a real `radiogroup` with a visible focus ring**: the
// mockup explicitly flags the absence of both as "a real accessibility
// defect" in the prototype — fixed here, not reproduced, same principle
// already followed for `SegmentedControl.vue`'s arrows.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { createTag, deleteTag, patchTag, type Tag } from '@/api/tags'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'

const TAG_SWATCH_HUES = [24, 150, 205, 340, 270, 34, 195, 0, 120, 290]
// The threshold isn't a 0-100% "confidence": it's `threshold * 100`
// directly, the same raw scale as OpenCLIP XLM-R IT/EN's real cosine
// score — scores of 0.10-0.20 even for correct matches, never close to
// 1.0. 20 matches the default in
// `migrations/0051_tag_threshold_default_openclip.sql`.
const DEFAULT_THRESHOLD_PERCENT = 20
const THRESHOLD_PERCENT_MIN = 5
const THRESHOLD_PERCENT_MAX = 40

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
    thresholdPercent.value = Math.round((props.tag.threshold ?? 0.2) * 100)
  } else {
    name.value = ''
    prompt.value = ''
    categoryId.value = ''
    color.value = swatchColor(TAG_SWATCH_HUES[props.tagCount % TAG_SWATCH_HUES.length])
    thresholdPercent.value = DEFAULT_THRESHOLD_PERCENT
  }
  nameError.value = ''
}

// `immediate: true`: the dialog can be born already open (same real bug
// already found in `ProblemFilesDialog.vue`) — without it, an editor
// opened directly in edit mode starts with empty fields instead of being
// prefilled from the tag.
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

// "Delete tag" closes *this* dialog first, then opens the page's own
// confirmation dialog — the two are never stacked.
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
          :min="THRESHOLD_PERCENT_MIN"
          :max="THRESHOLD_PERCENT_MAX"
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

<script setup lang="ts">
// **"N photos" shows `assignment_count`, not just confirmations**: the
// ideal number would only count `state==='confirmed'` rows, never pending
// proposals or rejections. The real backend does not expose that isolated
// number: `TagView.assignment_count` (`crates/keeppix-api/src/
// routes/tags.rs:41`) counts every `asset_tags` row for that tag, in any
// state — which is exactly the right number for the delete dialog ("will
// be removed from N photos", true for every row, decided or not), but
// slightly overstates the row label if pending proposals still exist. No
// route computes the confirmed-only subset on its own: building one would
// be a new route added purely for UI convenience, out of scope for a
// UI-only task. The number shown stays honest (real, not invented), just
// not filtered by state.
//
// **Duplicates are NOT allowed** (`UNIQUE(name, kind)` on the real
// backend) — the 409 error surfaces in the editor
// (`TagEditorDialog.vue`/`CategoryEditorDialog.vue`), not here.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { deleteTag, fetchTags, type Tag } from '@/api/tags'
import CategoryEditorDialog from '@/components/CategoryEditorDialog.vue'
import TagEditorDialog from '@/components/TagEditorDialog.vue'
import TagRow from '@/components/TagRow.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const toast = useToastStore()

const tags = ref<Tag[]>([])
const loading = ref(true)

async function load() {
  loading.value = true
  try {
    tags.value = await fetchTags()
  } catch {
    toast.showError(t('tags.loadError'))
  } finally {
    loading.value = false
  }
}

onMounted(load)

const categories = computed(() => tags.value.filter((tg) => tg.kind === 'category'))
const allTags = computed(() => tags.value.filter((tg) => tg.kind === 'tag'))

function tagsIn(categoryId: string): Tag[] {
  return allTags.value.filter((tg) => tg.parent_id === categoryId)
}

const orphanTags = computed(() =>
  allTags.value.filter((tg) => !tg.parent_id || !categories.value.some((c) => c.id === tg.parent_id))
)

const tagEditorOpen = ref(false)
const editingTag = ref<Tag | null>(null)

function openNewTag() {
  editingTag.value = null
  tagEditorOpen.value = true
}

function openEditTag(tag: Tag) {
  editingTag.value = tag
  tagEditorOpen.value = true
}

const categoryEditorOpen = ref(false)
const editingCategory = ref<Tag | null>(null)

function openNewCategory() {
  editingCategory.value = null
  categoryEditorOpen.value = true
}

function openEditCategory(category: Tag) {
  editingCategory.value = category
  categoryEditorOpen.value = true
}

const deleteTagTarget = ref<Tag | null>(null)
const deleteTagConfirmOpen = ref(false)

function askDeleteTag(tag: Tag) {
  deleteTagTarget.value = tag
  deleteTagConfirmOpen.value = true
}

async function confirmDeleteTag() {
  const target = deleteTagTarget.value
  if (!target) return
  try {
    await deleteTag(target.id)
    toast.show(t('tags.tagDeletedToast', { name: target.name }))
    await load()
  } catch {
    toast.showError(t('tags.deleteError'))
  }
}

const deleteCategoryTarget = ref<Tag | null>(null)
const deleteCategoryConfirmOpen = ref(false)

function askDeleteCategory(category: Tag) {
  deleteCategoryTarget.value = category
  deleteCategoryConfirmOpen.value = true
}

async function confirmDeleteCategory() {
  const target = deleteCategoryTarget.value
  if (!target) return
  try {
    await deleteTag(target.id)
    toast.show(t('tags.categoryDeletedToast', { name: target.name }))
    await load()
  } catch {
    toast.showError(t('tags.deleteError'))
  }
}
</script>

<template>
  <main class="mx-auto max-w-[720px] p-6">
    <p class="text-[15px] font-bold">
      {{ t('tags.title') }}
    </p>
    <p class="mt-1 text-[12.5px] text-content-muted">
      {{ t('tags.subtitle') }}
    </p>

    <div class="mt-4 flex gap-2">
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
        @click="openNewCategory"
      >
        {{ t('tags.newCategory') }}
      </button>
      <button
        type="button"
        class="rounded-lg bg-accent px-3 py-1.5 text-[13px] font-semibold text-accent-text"
        @click="openNewTag"
      >
        {{ t('tags.newTag') }}
      </button>
    </div>

    <section
      v-for="category in categories"
      :key="category.id"
      class="mt-6"
    >
      <div class="mb-2 flex items-center justify-between">
        <div class="flex items-baseline gap-2">
          <p class="text-[13.5px] font-bold">
            {{ category.name }}
          </p>
          <span class="text-[11.5px] text-content-muted">{{ t('tags.tagCount', { n: tagsIn(category.id).length }, { plural: tagsIn(category.id).length }) }}</span>
        </div>
        <div class="flex gap-1">
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[12px] text-content-muted hover:bg-border/30 hover:text-content"
            :aria-label="t('tags.renameCategory', { name: category.name })"
            @click="openEditCategory(category)"
          >
            {{ t('tags.edit') }}
          </button>
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[12px] text-danger hover:bg-danger/10"
            :aria-label="t('tags.deleteCategory', { name: category.name })"
            @click="askDeleteCategory(category)"
          >
            {{ t('tags.delete') }}
          </button>
        </div>
      </div>
      <div class="overflow-hidden rounded-xl border border-border">
        <p
          v-if="tagsIn(category.id).length === 0"
          class="px-3.5 py-3 text-[12.5px] text-content-muted"
        >
          {{ t('tags.emptyCategory') }}
        </p>
        <TagRow
          v-for="tag in tagsIn(category.id)"
          :key="tag.id"
          :tag="tag"
          @edit="openEditTag(tag)"
          @delete="askDeleteTag(tag)"
        />
      </div>
    </section>

    <section class="mt-6">
      <p class="mb-2 text-[13.5px] font-bold">
        {{ t('tags.noCategoryTitle') }}
      </p>
      <div class="overflow-hidden rounded-xl border border-border">
        <p
          v-if="orphanTags.length === 0"
          class="px-3.5 py-3 text-[12.5px] text-content-muted"
        >
          {{ t('tags.emptyOrphans') }}
        </p>
        <TagRow
          v-for="tag in orphanTags"
          :key="tag.id"
          :tag="tag"
          @edit="openEditTag(tag)"
          @delete="askDeleteTag(tag)"
        />
      </div>
    </section>

    <TagEditorDialog
      v-model:open="tagEditorOpen"
      :tag="editingTag"
      :categories="categories"
      :tag-count="allTags.length"
      @saved="load"
      @deleted="load"
    />
    <CategoryEditorDialog
      v-model:open="categoryEditorOpen"
      :category="editingCategory"
      @saved="load"
    />
    <ConfirmDialog
      v-if="deleteTagTarget"
      v-model:open="deleteTagConfirmOpen"
      :title="t('tags.deleteTagConfirmTitle', { name: deleteTagTarget.name })"
      :description="deleteTagTarget.assignment_count > 0
        ? t('tags.deleteTagConfirmDescriptionWithPhotos', { n: deleteTagTarget.assignment_count })
        : t('tags.deleteTagConfirmDescriptionEmpty')"
      :confirm-label="t('tags.delete')"
      @confirm="confirmDeleteTag"
    />
    <ConfirmDialog
      v-if="deleteCategoryTarget"
      v-model:open="deleteCategoryConfirmOpen"
      :title="t('tags.deleteCategoryConfirmTitle', { name: deleteCategoryTarget.name })"
      :description="t('tags.deleteCategoryConfirmDescription')"
      :confirm-label="t('tags.delete')"
      @confirm="confirmDeleteCategory"
    />
  </main>
</template>

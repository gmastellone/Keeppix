<script setup lang="ts">
import FolderBranch from '@/components/FolderBranch.vue'
import type { FolderChildren, FolderView } from '@/api/folders'

defineProps<{
  folder: FolderView
  kids: Record<string, FolderChildren>
  open: Set<string>
}>()

defineEmits<{
  toggle: [id: string]
  move: [folder: FolderView]
}>()
</script>

<template>
  <li>
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="rounded border border-border px-2 py-0.5 text-sm"
        :data-testid="`expand-${folder.id}`"
        :aria-expanded="open.has(folder.id)"
        @click="$emit('toggle', folder.id)"
      >
        {{ open.has(folder.id) ? '−' : '+' }}
      </button>
      <span>{{ folder.name }}</span>
      <button
        v-if="folder.parent_id"
        type="button"
        class="text-sm underline"
        :data-testid="`move-${folder.id}`"
        @click="$emit('move', folder)"
      >
        {{ $t('folders.move') }}
      </button>
    </div>
    <ul
      v-if="open.has(folder.id) && kids[folder.id]"
      class="ml-6 mt-1 space-y-1"
    >
      <FolderBranch
        v-for="child in kids[folder.id].folders"
        :key="child.id"
        :folder="child"
        :kids="kids"
        :open="open"
        @toggle="$emit('toggle', $event)"
        @move="$emit('move', $event)"
      />
      <li
        v-for="asset in kids[folder.id].assets"
        :key="asset.id"
        class="text-sm text-content-muted"
      >
        {{ asset.filename }}
      </li>
    </ul>
  </li>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { UploadSessionState } from '@/stores/upload'
import { useUploadStore } from '@/stores/upload'

// Nome a una parola per il file (combacia col piano), ma il componente ha
// comunque un nome di due parole: evita l'avviso `multi-word-component-names`
// senza doverlo disabilitare per un componente fuori da `ui/`.
defineOptions({ name: 'UploadPanel' })

const { t } = useI18n()
const store = useUploadStore()
const collapsed = ref(false)

onMounted(() => {
  void store.initFromStorage()
})

const visible = computed(() => store.sessions.length > 0)
const hasCompleted = computed(() =>
  store.sessions.some((s) => s.status === 'done' || s.status === 'skipped')
)

/** Solo formattazione di un numero — non è una stringa utente da tradurre,
 * la struttura "{received} of {total}" arriva da `upload.progress`. */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unitIndex]}`
}

function progressPercent(session: UploadSessionState): number {
  if (session.expectedSize <= 0) return 0
  return Math.min(100, Math.round((session.receivedBytes / session.expectedSize) * 100))
}

function statusLabel(session: UploadSessionState): string {
  if (session.status === 'done' && session.collision && session.collision !== 'created') {
    if (session.collision === 'renamed') {
      return t('upload.collision.renamed', { filename: session.savedFilename ?? session.filename })
    }
    return t('upload.collision.skipped_duplicate')
  }
  if (session.status === 'skipped') {
    return t('upload.collision.skipped_duplicate')
  }
  return t(`upload.status.${session.status}`)
}
</script>

<template>
  <aside
    v-if="visible"
    class="fixed bottom-4 right-4 z-50 w-80 rounded-lg border border-border bg-surface-elevated shadow-lg"
    role="region"
    :aria-label="t('upload.title')"
  >
    <div class="flex items-center justify-between border-b border-border px-3 py-2">
      <h2 class="text-sm font-medium text-content">
        {{ t('upload.title') }}
      </h2>
      <div class="flex items-center gap-2">
        <button
          v-if="hasCompleted"
          type="button"
          class="text-xs text-content-muted underline"
          @click="store.removeCompleted()"
        >
          {{ t('upload.clearCompleted') }}
        </button>
        <button
          type="button"
          class="text-xs text-content-muted underline"
          :aria-label="collapsed ? t('upload.expand') : t('upload.collapse')"
          @click="collapsed = !collapsed"
        >
          {{ collapsed ? t('upload.expand') : t('upload.collapse') }}
        </button>
      </div>
    </div>

    <ul
      v-if="!collapsed"
      class="max-h-72 overflow-y-auto"
    >
      <li
        v-for="session in store.sessions"
        :key="session.id"
        class="flex flex-col gap-1 border-b border-border px-3 py-2 last:border-b-0"
      >
        <div class="flex items-center gap-2">
          <span
            aria-hidden="true"
            class="shrink-0"
          >
            <template v-if="session.status === 'queued'">⏳</template>
            <template v-else-if="session.status === 'uploading'">↑</template>
            <template v-else-if="session.status === 'paused'">⏸</template>
            <template v-else-if="session.status === 'done' || session.status === 'skipped'">✓</template>
            <template v-else-if="session.status === 'error'">✕</template>
          </span>
          <span class="min-w-0 flex-1 truncate text-sm text-content">{{ session.filename }}</span>
        </div>

        <div
          v-if="session.status === 'uploading' || session.status === 'paused'"
          class="h-1.5 w-full overflow-hidden rounded-full bg-surface"
        >
          <div
            class="h-full bg-accent"
            :style="{ width: `${progressPercent(session)}%` }"
          />
        </div>

        <div class="flex items-center justify-between text-xs text-content-muted">
          <span>{{ statusLabel(session) }}</span>
          <span v-if="session.status === 'uploading' || session.status === 'paused'">
            {{
              t('upload.progress', {
                received: formatBytes(session.receivedBytes),
                total: formatBytes(session.expectedSize)
              })
            }}
          </span>
        </div>

        <div
          v-if="session.status === 'error' || session.status === 'uploading' || session.status === 'paused'"
          class="flex gap-2"
        >
          <button
            v-if="session.status === 'error'"
            type="button"
            class="text-xs text-accent underline"
            @click="store.retry(session.id)"
          >
            {{ t('upload.retry') }}
          </button>
          <button
            v-if="session.status === 'uploading'"
            type="button"
            class="text-xs text-accent underline"
            @click="store.pause(session.id)"
          >
            {{ t('upload.pause') }}
          </button>
          <button
            v-if="session.status === 'paused'"
            type="button"
            class="text-xs text-accent underline"
            @click="store.resume(session.id)"
          >
            {{ t('upload.resume') }}
          </button>
        </div>
      </li>
    </ul>
  </aside>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { readAndClearSharedFiles } from '@/pwa/shareTarget'
import { useUploadStore } from '@/stores/upload'

// Vista di passaggio: il service worker (`public/sw.js`) ha già intercettato
// il POST di condivisione dell'OS e ridiretto qui via GET. Non c'è UI reale
// da mostrare: si legge la coda, si passa allo store di upload (Task 3) e
// si torna alla home, dove il pannello di upload globale (montato in
// `App.vue`) mostra i file appena accodati.
defineOptions({ name: 'ShareTargetView' })

const { t } = useI18n()
const router = useRouter()
const upload = useUploadStore()

onMounted(async () => {
  const files = await readAndClearSharedFiles()
  if (files.length > 0) {
    await upload.addSharedFiles(files)
  }
  await router.replace('/')
})
</script>

<template>
  <main
    class="mx-auto flex min-h-full max-w-sm flex-col items-center justify-center gap-4 p-6 text-center"
  >
    <p class="text-content-muted">
      {{ t('common.loading') }}
    </p>
  </main>
</template>

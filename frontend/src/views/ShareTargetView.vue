<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { readAndClearSharedFiles } from '@/pwa/shareTarget'
import { useUploadStore } from '@/stores/upload'

// A pass-through view: the service worker (`public/sw.js`) has already
// intercepted the OS's share POST and redirected here via GET. There's no
// real UI to show: it reads the queue, hands it off to the upload store,
// and returns to the home page, where the global upload panel (mounted in
// `App.vue`) shows the files that were just queued.
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

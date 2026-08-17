<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import { fetchLibraryScanStatus, startLibraryScan } from '@/api/libraries'
import type { ScanStatus } from '@/api/libraries'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'

const POLL_MS = 2000

const props = defineProps<{ libraryId: string }>()

const { t } = useI18n()
const router = useRouter()

const status = ref<ScanStatus | null>(null)
const error = ref('')
const starting = ref(true)
let timer: ReturnType<typeof setInterval> | undefined

const phaseLabel = computed(() => {
  const phase = status.value?.phase ?? 'discovering'
  const key = `setup.scan.phase.${phase}` as const
  const translated = t(key)
  return translated === key ? t('setup.scan.phase.discovering') : translated
})

function messageForPoll(e: unknown): string {
  if (e instanceof ApiProblem) {
    if (e.type === 'keeppix/service-unavailable' || e.status === 503) {
      return t('common.unavailable')
    }
    if (e.type === 'keeppix/unauthenticated' || e.status === 401) {
      return t('setup.scan.errors.unauthenticated')
    }
  }
  return t('common.unexpectedError')
}

async function poll() {
  try {
    status.value = await fetchLibraryScanStatus(props.libraryId)
    error.value = ''
    if (status.value.phase === 'idle' && status.value.asset_count > 0) {
      await router.push('/')
    }
  } catch (e) {
    error.value = messageForPoll(e)
  }
}

async function start() {
  starting.value = true
  error.value = ''
  try {
    await startLibraryScan(props.libraryId)
    await poll()
  } catch (e) {
    error.value =
      e instanceof ApiProblem && e.status === 401
        ? t('setup.scan.errors.unauthenticated')
        : t('setup.scan.errors.startFailed')
  } finally {
    starting.value = false
  }
}

function goHome() {
  void router.push('/')
}

onMounted(() => {
  void start()
  timer = setInterval(() => {
    void poll()
  }, POLL_MS)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex flex-col gap-1">
      <h2 class="text-xl font-semibold">
        {{ t('setup.scan.title') }}
      </h2>
      <p class="text-sm text-content-muted">
        {{ t('setup.scan.subtitle') }}
      </p>
    </header>

    <div
      v-if="status || starting"
      class="rounded-lg border border-border bg-surface-elevated p-4 text-sm"
    >
      <p class="font-medium">
        {{ phaseLabel }}
      </p>
      <p
        v-if="status"
        class="mt-2 text-content-muted"
      >
        {{ t('setup.scan.assetCount', { count: status.asset_count }) }}
      </p>
    </div>

    <Alert
      v-if="error"
      :message="error"
    />

    <Button
      v-if="status?.phase === 'idle'"
      type="button"
      @click="goHome"
    >
      {{ t('setup.scan.goToPhotos') }}
    </Button>
  </section>
</template>

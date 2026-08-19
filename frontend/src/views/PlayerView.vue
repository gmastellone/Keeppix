<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { apiFetch } from '@/api/client'

type PlaybackPlan = {
  mode: 'direct' | 'hls'
  url: string
  poster_url: string
  ready: boolean
}

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const videoEl = ref<HTMLVideoElement | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)
const saveBandwidth = ref(false)
let hlsInstance: { destroy: () => void } | null = null
let pollTimer: ReturnType<typeof setTimeout> | undefined

async function loadPlayback() {
  loading.value = true
  error.value = null
  destroyPlayer()
  const id = route.params.id as string
  try {
    const plan = await apiFetch<PlaybackPlan>(
      `/media/video/${id}/playback?save_bandwidth=${saveBandwidth.value ? 'true' : 'false'}`
    )
    if (plan.mode === 'direct' && plan.ready) {
      await attachDirect(plan.url, plan.poster_url)
      loading.value = false
      return
    }
    if (!plan.ready) {
      await waitForHls(plan)
      return
    }
    await attachHls(plan.url, plan.poster_url)
    loading.value = false
  } catch {
    error.value = t('player.error')
    loading.value = false
  }
}

async function waitForHls(_plan: PlaybackPlan) {
  const id = route.params.id as string
  const poll = async () => {
    const next = await apiFetch<PlaybackPlan>(
      `/media/video/${id}/playback?save_bandwidth=${saveBandwidth.value ? 'true' : 'false'}`
    )
    if (next.ready) {
      await attachHls(next.url, next.poster_url)
      loading.value = false
      return
    }
    pollTimer = setTimeout(() => { void poll() }, 1500)
  }
  pollTimer = setTimeout(() => { void poll() }, 1500)
}

async function attachDirect(url: string, poster: string) {
  const el = videoEl.value
  if (!el) return
  el.poster = poster
  el.src = url
  await el.play().catch(() => { /* autoplay may be blocked */ })
}

async function attachHls(url: string, poster: string) {
  const el = videoEl.value
  if (!el) return
  el.poster = poster
  if (el.canPlayType('application/vnd.apple.mpegurl')) {
    el.src = url
    await el.play().catch(() => { /* autoplay may be blocked */ })
    return
  }
  const { default: Hls } = await import('hls.js')
  if (!Hls.isSupported()) {
    error.value = t('player.unsupported')
    return
  }
  const hls = new Hls()
  hls.loadSource(url)
  hls.attachMedia(el)
  hlsInstance = hls
  await el.play().catch(() => { /* autoplay may be blocked */ })
}

function destroyPlayer() {
  if (pollTimer) {
    clearTimeout(pollTimer)
    pollTimer = undefined
  }
  hlsInstance?.destroy()
  hlsInstance = null
  const el = videoEl.value
  if (el) {
    el.removeAttribute('src')
    el.load()
  }
}

function close() {
  router.back()
}

onMounted(() => { void loadPlayback() })
onUnmounted(() => destroyPlayer())
watch(() => route.params.id, () => { void loadPlayback() })
watch(saveBandwidth, () => { void loadPlayback() })
</script>

<template>
  <div class="fixed inset-0 z-50 flex flex-col bg-black text-white">
    <header class="flex items-center justify-between gap-3 px-4 py-3">
      <button type="button" class="text-sm opacity-80 hover:opacity-100" @click="close">
        {{ t('player.close') }}
      </button>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="saveBandwidth" type="checkbox" />
        {{ t('player.saveBandwidth') }}
      </label>
    </header>
    <div class="flex flex-1 items-center justify-center px-4 pb-8">
      <p v-if="loading" class="text-sm opacity-70">{{ t('player.loading') }}</p>
      <p v-else-if="error" class="text-sm text-red-300">{{ error }}</p>
      <video
        v-show="!loading && !error"
        ref="videoEl"
        class="max-h-full max-w-full"
        controls
        playsinline
      />
    </div>
  </div>
</template>

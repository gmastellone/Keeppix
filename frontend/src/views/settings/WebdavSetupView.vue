<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  createAppPassword,
  deleteAppPassword,
  listAppPasswords,
  type AppPassword,
  type AppPasswordCreated
} from '@/api/appPasswords'
import { useSessionStore } from '@/stores/session'

const { t, locale } = useI18n()
const session = useSessionStore()

/** Lightweight polling on `last_used_at`, not a new protocol. */
const POLL_INTERVAL_MS = 3000
const POLL_TIMEOUT_MS = 5 * 60 * 1000

const label = ref(t('webdav.defaultLabel'))
const generating = ref(false)
const generateError = ref(false)
const created = ref<AppPasswordCreated | null>(null)
const copied = ref(false)
const connected = ref(false)

const previous = ref<AppPassword[]>([])
const loadError = ref(false)
const revokeError = ref(false)

let pollTimer: ReturnType<typeof setInterval> | undefined
let pollDeadline = 0

const davUrl = computed(() => `${window.location.origin}/dav/`)

const rcloneConfig = computed(() => {
  if (!created.value) return ''
  return [
    '[keeppix]',
    'type = webdav',
    `url = ${davUrl.value}`,
    'vendor = other',
    `user = ${session.user?.username ?? created.value.label}`,
    'pass = ***'
  ].join('\n')
})

function formatDate(iso: string): string {
  return new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium' }).format(new Date(iso))
}

async function loadPrevious(): Promise<void> {
  loadError.value = false
  try {
    previous.value = await listAppPasswords()
  } catch {
    loadError.value = true
  }
}

function stopPolling(): void {
  if (pollTimer) clearInterval(pollTimer)
  pollTimer = undefined
}

async function pollStatus(): Promise<void> {
  const current = created.value
  if (!current) {
    stopPolling()
    return
  }
  try {
    const list = await listAppPasswords()
    previous.value = list
    const match = list.find((entry) => entry.id === current.id)
    if (match?.last_used_at) {
      connected.value = true
      stopPolling()
      return
    }
  } catch {
    // Transient network error: keep polling until the timeout.
  }
  if (Date.now() >= pollDeadline) {
    stopPolling()
  }
}

function startPolling(): void {
  stopPolling()
  pollDeadline = Date.now() + POLL_TIMEOUT_MS
  pollTimer = setInterval(() => void pollStatus(), POLL_INTERVAL_MS)
}

async function generate(): Promise<void> {
  if (generating.value) return
  generating.value = true
  generateError.value = false
  try {
    const trimmed = label.value.trim() || t('webdav.defaultLabel')
    created.value = await createAppPassword(trimmed)
    connected.value = false
    copied.value = false
    startPolling()
  } catch {
    generateError.value = true
  } finally {
    generating.value = false
  }
}

async function copySecret(): Promise<void> {
  if (!created.value) return
  try {
    await navigator.clipboard.writeText(created.value.secret)
    copied.value = true
  } catch {
    copied.value = false
  }
}

async function revoke(id: string): Promise<void> {
  revokeError.value = false
  try {
    await deleteAppPassword(id)
    await loadPrevious()
  } catch {
    revokeError.value = true
  }
}

onMounted(() => {
  void loadPrevious()
})

onBeforeUnmount(stopPolling)
</script>

<template>
  <section class="mx-auto max-w-2xl space-y-8 p-6">
    <header>
      <h1 class="text-2xl font-semibold">
        {{ t('webdav.title') }}
      </h1>
      <p class="mt-1 text-sm text-content-muted">
        {{ t('webdav.subtitle') }}
      </p>
    </header>

    <section v-if="!created">
      <form
        class="grid gap-3 rounded-lg border border-border bg-surface-elevated p-4"
        @submit.prevent="generate"
      >
        <label class="text-sm">
          <span class="mb-1 block">{{ t('webdav.labelField') }}</span>
          <input
            v-model.trim="label"
            name="app-password-label"
            class="w-full rounded border border-border bg-surface px-2 py-1.5"
          >
        </label>
        <button
          type="button"
          class="rounded bg-accent px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          data-action="generate-app-password"
          :disabled="generating"
          @click="generate"
        >
          {{ t('webdav.generate') }}
        </button>
      </form>
      <p
        v-if="generateError"
        class="mt-2 text-sm text-danger"
        role="alert"
      >
        {{ t('webdav.errors.generateFailed') }}
      </p>
    </section>

    <section
      v-else
      class="space-y-6"
    >
      <div class="rounded-lg border border-border bg-surface-elevated p-4">
        <h2 class="font-medium">
          {{ t('webdav.generateTitle') }}
        </h2>
        <p class="mt-2 break-all font-mono text-sm">
          {{ created.label }} · {{ created.secret }}
        </p>
        <button
          type="button"
          class="mt-2 rounded border border-border px-3 py-1.5 text-sm"
          data-action="copy-secret"
          @click="copySecret"
        >
          {{ copied ? t('webdav.copied') : t('webdav.copy') }}
        </button>
        <p class="mt-2 text-sm text-danger">
          {{ t('webdav.secretWarning') }}
        </p>
      </div>

      <p class="text-sm">
        <span
          v-if="connected"
          class="text-success"
        >✅ {{ t('webdav.status.connected') }}</span>
        <span
          v-else
          class="text-content-muted"
        >{{ t('webdav.status.waiting') }}</span>
      </p>

      <div class="space-y-3">
        <details
          class="rounded-lg border border-border p-3"
          open
        >
          <summary class="cursor-pointer font-medium">
            {{ t('webdav.clients.macos.title') }}
          </summary>
          <p class="mt-2 text-sm">
            {{ t('webdav.clients.macos.steps', { url: davUrl }) }}
          </p>
        </details>
        <details class="rounded-lg border border-border p-3">
          <summary class="cursor-pointer font-medium">
            {{ t('webdav.clients.windows.title') }}
          </summary>
          <p class="mt-2 text-sm">
            {{ t('webdav.clients.windows.hint') }}
          </p>
          <p class="mt-1 text-sm text-danger">
            {{ t('webdav.clients.windows.limitWarning') }}
          </p>
        </details>
        <details class="rounded-lg border border-border p-3">
          <summary class="cursor-pointer font-medium">
            {{ t('webdav.clients.rclone.title') }}
          </summary>
          <p class="mt-2 text-sm">
            {{ t('webdav.clients.rclone.hint') }}
          </p>
          <pre class="mt-2 overflow-x-auto rounded bg-surface p-2 text-xs">{{ rcloneConfig }}</pre>
        </details>
        <details class="rounded-lg border border-border p-3">
          <summary class="cursor-pointer font-medium">
            {{ t('webdav.clients.mobile.title') }}
          </summary>
          <p class="mt-2 text-sm">
            {{ t('webdav.clients.mobile.hint') }}
          </p>
          <p class="mt-1 break-all font-mono text-xs">
            {{ davUrl }}
          </p>
        </details>
      </div>
    </section>

    <section>
      <h2 class="mb-2 font-medium">
        {{ t('webdav.previous.title') }}
      </h2>
      <p
        v-if="loadError"
        class="text-sm text-danger"
        role="alert"
      >
        {{ t('common.unexpectedError') }}
      </p>
      <p
        v-else-if="previous.length === 0"
        class="text-sm text-content-muted"
      >
        {{ t('webdav.previous.empty') }}
      </p>
      <ul
        v-else
        class="space-y-2"
      >
        <li
          v-for="entry in previous"
          :key="entry.id"
          class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border px-3 py-2"
        >
          <div>
            <span class="font-medium">{{ entry.label }}</span>
            <span class="ml-2 text-xs text-content-muted">
              {{
                entry.last_used_at
                  ? t('webdav.previous.usedOn', { date: formatDate(entry.last_used_at) })
                  : t('webdav.previous.neverUsed')
              }}
            </span>
          </div>
          <button
            type="button"
            class="text-sm text-danger underline"
            :data-action="`revoke-${entry.id}`"
            @click="revoke(entry.id)"
          >
            {{ t('webdav.previous.revoke') }}
          </button>
        </li>
      </ul>
      <p
        v-if="revokeError"
        class="mt-2 text-sm text-danger"
        role="alert"
      >
        {{ t('webdav.errors.revokeFailed') }}
      </p>
    </section>
  </section>
</template>

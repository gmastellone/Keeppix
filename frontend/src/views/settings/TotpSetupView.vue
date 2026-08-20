<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  beginTotpSetup,
  confirmTotp,
  disableTotp,
  getTotpStatus,
  regenerateTotpRecoveryCodes,
  type TotpSetup,
  type TotpStatus
} from '@/api/totp'
import { ApiProblem } from '@/api/client'

const { t } = useI18n()

const status = ref<TotpStatus | null>(null)
const setup = ref<TotpSetup | null>(null)
const recoveryCodes = ref<string[] | null>(null)
const code = ref('')
const actionCode = ref('')
const loading = ref(false)
const loadError = ref(false)
const actionError = ref('')
const copied = ref(false)

async function refreshStatus(): Promise<void> {
  loadError.value = false
  try {
    status.value = await getTotpStatus()
  } catch {
    loadError.value = true
  }
}

async function startSetup(): Promise<void> {
  loading.value = true
  actionError.value = ''
  recoveryCodes.value = null
  try {
    setup.value = await beginTotpSetup()
    code.value = ''
    await refreshStatus()
  } catch (e) {
    actionError.value =
      e instanceof ApiProblem && e.type === 'keeppix/conflict'
        ? t('totp.errors.alreadyEnabled')
        : t('totp.errors.setupFailed')
  } finally {
    loading.value = false
  }
}

async function confirm(): Promise<void> {
  if (!code.value.trim()) return
  loading.value = true
  actionError.value = ''
  try {
    const result = await confirmTotp(code.value.trim())
    recoveryCodes.value = result.recovery_codes
    setup.value = null
    code.value = ''
    await refreshStatus()
  } catch {
    actionError.value = t('totp.errors.confirmFailed')
  } finally {
    loading.value = false
  }
}

async function regenerate(): Promise<void> {
  if (!actionCode.value.trim()) return
  loading.value = true
  actionError.value = ''
  try {
    const result = await regenerateTotpRecoveryCodes(actionCode.value.trim())
    recoveryCodes.value = result.recovery_codes
    actionCode.value = ''
    await refreshStatus()
  } catch {
    actionError.value = t('totp.errors.regenerateFailed')
  } finally {
    loading.value = false
  }
}

async function disable(): Promise<void> {
  if (!actionCode.value.trim()) return
  loading.value = true
  actionError.value = ''
  try {
    await disableTotp(actionCode.value.trim())
    recoveryCodes.value = null
    setup.value = null
    actionCode.value = ''
    await refreshStatus()
  } catch {
    actionError.value = t('totp.errors.disableFailed')
  } finally {
    loading.value = false
  }
}

async function copyRecovery(): Promise<void> {
  if (!recoveryCodes.value) return
  try {
    await navigator.clipboard.writeText(recoveryCodes.value.join('\n'))
    copied.value = true
  } catch {
    copied.value = false
  }
}

onMounted(() => {
  void refreshStatus()
})
</script>

<template>
  <section class="mx-auto max-w-2xl space-y-8 p-6">
    <header>
      <h1 class="text-2xl font-semibold">
        {{ t('totp.title') }}
      </h1>
      <p class="mt-1 text-sm text-content-muted">
        {{ t('totp.subtitle') }}
      </p>
    </header>

    <p
      v-if="loadError"
      class="text-sm text-danger"
      role="alert"
    >
      {{ t('common.unexpectedError') }}
    </p>

    <section
      v-else-if="status"
      class="space-y-6"
    >
      <p class="text-sm">
        <span v-if="status.enabled">{{ t('totp.status.enabled') }}</span>
        <span v-else-if="status.pending">{{ t('totp.status.pending') }}</span>
        <span v-else>{{ t('totp.status.off') }}</span>
        <span
          v-if="status.enabled"
          class="ml-2 text-content-muted"
        >
          {{
            t('totp.status.recoveryLeft', {
              count: status.unused_recovery_codes
            })
          }}
        </span>
      </p>

      <div
        v-if="!status.enabled && !setup"
        class="space-y-3"
      >
        <p class="text-sm text-content-muted">
          {{ t('totp.intro') }}
        </p>
        <button
          type="button"
          class="rounded bg-accent px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          data-action="totp-start"
          :disabled="loading"
          @click="startSetup"
        >
          {{ t('totp.start') }}
        </button>
      </div>

      <div
        v-if="setup"
        class="space-y-4 rounded-lg border border-border bg-surface-elevated p-4"
      >
        <h2 class="font-medium">
          {{ t('totp.scanTitle') }}
        </h2>
        <img
          class="mx-auto w-52"
          data-role="totp-qr"
          :src="`data:image/svg+xml;utf8,${encodeURIComponent(setup.qr_svg)}`"
          :alt="t('totp.scanTitle')"
        >
        <p class="break-all font-mono text-xs text-content-muted">
          {{ setup.secret_base32 }}
        </p>
        <p class="text-sm">
          {{ t('totp.manualHint') }}
        </p>
        <label class="block text-sm">
          <span class="mb-1 block">{{ t('totp.codeLabel') }}</span>
          <input
            v-model.trim="code"
            name="totp-confirm-code"
            inputmode="numeric"
            autocomplete="one-time-code"
            maxlength="6"
            class="w-full rounded border border-border bg-surface px-2 py-1.5 font-mono tracking-widest"
          >
        </label>
        <button
          type="button"
          class="rounded bg-accent px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          data-action="totp-confirm"
          :disabled="loading || code.length !== 6"
          @click="confirm"
        >
          {{ t('totp.confirm') }}
        </button>
      </div>

      <div
        v-if="recoveryCodes"
        class="space-y-3 rounded-lg border border-border bg-surface-elevated p-4"
      >
        <h2 class="font-medium">
          {{ t('totp.recoveryTitle') }}
        </h2>
        <p class="text-sm text-danger">
          {{ t('totp.recoveryWarning') }}
        </p>
        <ul class="grid gap-1 font-mono text-sm">
          <li
            v-for="entry in recoveryCodes"
            :key="entry"
          >
            {{ entry }}
          </li>
        </ul>
        <button
          type="button"
          class="rounded border border-border px-3 py-1.5 text-sm"
          data-action="totp-copy-recovery"
          @click="copyRecovery"
        >
          {{ copied ? t('totp.copied') : t('totp.copy') }}
        </button>
      </div>

      <div
        v-if="status.enabled"
        class="space-y-4 rounded-lg border border-border p-4"
      >
        <h2 class="font-medium">
          {{ t('totp.manageTitle') }}
        </h2>
        <label class="block text-sm">
          <span class="mb-1 block">{{ t('totp.actionCodeLabel') }}</span>
          <input
            v-model.trim="actionCode"
            name="totp-action-code"
            class="w-full rounded border border-border bg-surface px-2 py-1.5 font-mono"
          >
        </label>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            class="rounded border border-border px-3 py-1.5 text-sm disabled:opacity-50"
            data-action="totp-regenerate"
            :disabled="loading || !actionCode"
            @click="regenerate"
          >
            {{ t('totp.regenerate') }}
          </button>
          <button
            type="button"
            class="rounded border border-danger px-3 py-1.5 text-sm text-danger disabled:opacity-50"
            data-action="totp-disable"
            :disabled="loading || !actionCode"
            @click="disable"
          >
            {{ t('totp.disable') }}
          </button>
        </div>
      </div>

      <p
        v-if="actionError"
        class="text-sm text-danger"
        role="alert"
      >
        {{ actionError }}
      </p>
    </section>
  </section>
</template>

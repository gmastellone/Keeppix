<script setup lang="ts">
// Four sections, each with real behavior:
//
// - **"Account data"**: only display name actually writes
//   (`session.updateDisplayName`, `PATCH /users/{id}`). "Email" stays
//   read-only: `UserView` exposes it but no route writes it — the field
//   honestly declares that limit instead of accepting an edit that would
//   silently disappear.
// - **"Avatar color"**: real, but stored in `localStorage` per `user.id`
//   (`stores/avatarColor.ts`) — there is no server-side preference for
//   this field, see the comment in that file.
// - **"Security"**: "Change password" opens a real form
//   (`ChangePasswordDialog.vue`, `POST /users/me/password`). Two-factor
//   authentication is not a mini-switch that just flips a flag: it's the
//   full flow already built in `TotpSetupView.vue`
//   (`/settings/security/totp`) — here it's just real status
//   (`GET /auth/totp`) plus a link, nothing reinvented.
// - **"Active sessions"**: real, and a list of real length
//   (`GET /users/me/sessions`,
//   `crates/keeppix-api/src/routes/sessions.rs`). "Log out" and "Log out
//   from all other devices" work for real. `device_label` comes from the
//   user agent at login, always in English (e.g. "Chrome on macOS").
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchSessions, revokeOtherSessions, revokeSession, type SessionView } from '@/api/sessions'
import { getTotpStatus, type TotpStatus } from '@/api/totp'
import ChangePasswordDialog from '@/components/ChangePasswordDialog.vue'
import Avatar from '@/components/ui/Avatar.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { AVATAR_COLOR_OPTIONS, useAvatarColorStore } from '@/stores/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

const { t, locale } = useI18n()
const session = useSessionStore()
const avatarColor = useAvatarColorStore()
const toast = useToastStore()

const roleLabel = computed(() => t(session.user?.role === 'admin' ? 'users.roleAdmin' : 'users.roleUser'))

const displayName = ref(session.user?.display_name ?? '')
const savingAccount = ref(false)

async function saveAccount() {
  savingAccount.value = true
  try {
    await session.updateDisplayName(displayName.value)
    toast.show(t('profile.account.saveDone'))
  } catch {
    toast.showError(t('profile.account.saveError'))
  } finally {
    savingAccount.value = false
  }
}

function selectAvatarColor(id: string) {
  if (!session.user) return
  avatarColor.setColor(session.user.id, id)
}

const totp = ref<TotpStatus | null>(null)
const passwordDialogOpen = ref(false)

const sessions = ref<SessionView[]>([])
const sessionsBusy = ref<Set<string>>(new Set())
const revokeOthersOpen = ref(false)

function formatLastSeen(iso: string): string {
  const rtf = new Intl.RelativeTimeFormat(locale.value, { numeric: 'auto' })
  const diffMin = Math.round((Date.now() - new Date(iso).getTime()) / 60_000)
  if (Math.abs(diffMin) < 60) return rtf.format(-diffMin, 'minute')
  const diffHours = Math.round(diffMin / 60)
  if (Math.abs(diffHours) < 24) return rtf.format(-diffHours, 'hour')
  return rtf.format(-Math.round(diffHours / 24), 'day')
}

async function loadSessions() {
  try {
    sessions.value = await fetchSessions()
  } catch {
    // No sessions listed (network issue, session just expired): the rest
    // of the page still remains usable.
  }
}

async function logoutSession(row: SessionView) {
  if (sessionsBusy.value.has(row.id)) return
  sessionsBusy.value = new Set(sessionsBusy.value).add(row.id)
  try {
    await revokeSession(row.id)
    sessions.value = sessions.value.filter((s) => s.id !== row.id)
    toast.show(t('profile.sessions.revokeDone'))
  } catch {
    toast.showError(t('profile.sessions.actionError'))
  } finally {
    const next = new Set(sessionsBusy.value)
    next.delete(row.id)
    sessionsBusy.value = next
  }
}

async function confirmLogoutOthers() {
  try {
    await revokeOtherSessions()
    sessions.value = sessions.value.filter((s) => s.current)
    toast.show(t('profile.sessions.logoutOthersDone'))
  } catch {
    toast.showError(t('profile.sessions.actionError'))
  }
}

onMounted(async () => {
  await Promise.all([
    getTotpStatus()
      .then((s) => (totp.value = s))
      .catch(() => {}),
    loadSessions()
  ])
})
</script>

<template>
  <main class="mx-auto max-w-[560px] p-6">
    <div class="flex items-center gap-3">
      <Avatar
        :name="session.user?.display_name ?? ''"
        :color="avatarColor.hex"
        size="lg"
      />
      <div>
        <p class="text-[17px] font-bold">
          {{ session.user?.display_name }}
        </p>
        <p class="text-sm text-content-muted">
          {{ roleLabel }} · {{ t('profile.subtitleServer') }}
        </p>
      </div>
    </div>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('profile.account.title') }}
      </p>
      <div class="mt-2.5 flex flex-col gap-3">
        <TextField
          v-model="displayName"
          :label="t('profile.account.displayName')"
        />
        <div class="flex flex-col gap-1.5">
          <label class="text-sm font-medium text-content">{{ t('profile.account.email') }}</label>
          <input
            :value="session.user?.email ?? ''"
            disabled
            class="rounded-lg border border-border bg-surface px-3 py-2.5 text-content-muted"
          >
          <p class="text-xs text-content-muted">
            {{ t('profile.account.emailReadonly') }}
          </p>
        </div>
        <button
          type="button"
          class="w-fit rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white disabled:opacity-60"
          :disabled="savingAccount"
          @click="saveAccount"
        >
          {{ t('profile.account.save') }}
        </button>
      </div>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('profile.avatarColor.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('profile.avatarColor.subtitle') }}
      </p>
      <div class="mt-2.5 flex flex-wrap gap-2">
        <button
          v-for="opt in AVATAR_COLOR_OPTIONS"
          :key="opt.id"
          type="button"
          class="relative h-[30px] w-[30px] shrink-0 rounded-full"
          :style="{ background: opt.hex ?? 'var(--color-accent)' }"
          :aria-label="opt.label"
          :aria-pressed="avatarColor.colorId === opt.id"
          @click="selectAvatarColor(opt.id)"
        >
          <span
            v-if="avatarColor.colorId === opt.id"
            class="absolute inset-0 flex items-center justify-center text-white"
          >✓</span>
        </button>
      </div>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('profile.security.title') }}
      </p>
      <div class="mt-2.5 flex flex-col gap-3">
        <div class="flex items-center justify-between">
          <span class="text-[13px]">{{ t('profile.security.passwordLabel') }}</span>
          <button
            type="button"
            class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold hover:bg-border/20"
            @click="passwordDialogOpen = true"
          >
            {{ t('profile.security.changePassword') }}
          </button>
        </div>
        <div class="flex items-center justify-between gap-3">
          <div>
            <p class="text-[13px]">
              {{ t('profile.security.twoFactorLabel') }}
            </p>
            <p class="text-xs text-content-muted">
              {{ t('profile.security.twoFactorSubtitle') }}
            </p>
          </div>
          <RouterLink
            to="/settings/security/totp"
            class="shrink-0 rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold hover:bg-border/20"
          >
            {{ totp?.enabled ? t('profile.security.twoFactorManage') : t('profile.security.twoFactorEnable') }}
          </RouterLink>
        </div>
      </div>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('profile.sessions.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('profile.sessions.subtitle') }}
      </p>
      <ul class="mt-2.5 flex flex-col gap-2.5">
        <li
          v-for="row in sessions"
          :key="row.id"
          class="flex items-center justify-between gap-2"
        >
          <div>
            <p class="text-[13px]">
              {{ row.device_label ?? t('profile.sessions.unknownDevice') }}<span
                v-if="row.current"
                class="font-bold text-accent"
              >{{ t('profile.sessions.currentSuffix') }}</span>
            </p>
            <p class="text-xs text-content-muted">
              {{ row.current ? t('profile.sessions.activeNow') : formatLastSeen(row.last_seen_at) }}
            </p>
          </div>
          <button
            v-if="!row.current"
            type="button"
            :disabled="sessionsBusy.has(row.id)"
            class="shrink-0 rounded-lg border border-border px-3 py-1.5 text-[12.5px] font-semibold hover:bg-border/20 disabled:opacity-60"
            @click="logoutSession(row)"
          >
            {{ t('profile.sessions.logout') }}
          </button>
        </li>
      </ul>
      <button
        v-if="sessions.length > 1"
        type="button"
        class="mt-3.5 rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10"
        @click="revokeOthersOpen = true"
      >
        {{ t('profile.sessions.logoutOthers') }}
      </button>
    </section>

    <ChangePasswordDialog v-model:open="passwordDialogOpen" />
    <ConfirmDialog
      v-model:open="revokeOthersOpen"
      :title="t('profile.sessions.logoutOthersConfirmTitle')"
      :description="t('profile.sessions.logoutOthersConfirmDescription')"
      :confirm-label="t('profile.sessions.logoutOthersConfirmButton')"
      @confirm="confirmLogoutOthers"
    />
  </main>
</template>

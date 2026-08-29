<script setup lang="ts">
// A flat list with NO accordion, unlike the desktop sidebar, which uses
// `NavGroup` — not reused here on purpose: all rows appear already open,
// no group to expand.
//
// Scope: the same real destinations as AppSidebar (Favorites and Persons
// included), not a fixed set of generic groups.
//
// "Persons" lives here under "Library" ("from mobile 'More' → 'Library'
// group → 'Persons'"), **not** in `NAV_TOP` like on desktop
// (`AppSidebar.vue`) — a per-platform placement difference, not a
// divergence introduced here.
//
// The "AI" group has two real entries: "Tags and categories" and
// "Review" (badge `shell.badges.revision`, same data as
// `AppSidebar.vue`). "Library analysis" stays out permanently, not just
// for now: no route reads it (same extended comment in
// `AppSidebar.vue`).
// - "Shared with me" / "My shares" as two distinct rows: `SharesView`
//   has no separate tabs for that — it's a single view, collapsed into
//   one "Shares" row pointing to `/shares`.
// - No secondary "N folders" value on the "Folders" row: no count is
//   available without a dedicated call just for that badge (the same
//   reason `FolderView` carries no photo count).
// - No "Folders" sub-page with gradient cards (cover from the first
//   photo, photo count): that sub-page doesn't exist here, "Folders"
//   goes directly to `/folders` (the real folder tree).
// Added, not part of any mockup: "Administration" (Users/Groups, only
// for `role==='admin'`) — same reason as AppSidebar, a real feature of
// the multi-user backend.
//
// No icons: this frontend doesn't have an icon system yet (same state of
// affairs as AppSidebar). Every row is a real `<RouterLink>`, keyboard
// reachable by construction.
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { startLiveEvents, type LiveSocket } from '@/api/events'
import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const session = useSessionStore()
const shell = useShellStore()

// Added: a view-only indicator for the two automatic operations with no
// user-initiated trigger — `AiAnalysis`/`FaceDetection` (background
// windows, never an HTTP route, unlike `LibraryScan`/`BulkRename`). No
// pause/cancel button on purpose: this is just visibility into what's
// happening in the background, not a control. `operation.progress`
// carries no `kind` field — the type is read from the `phase` string,
// the only field the two jobs actually set (`embed.rs`: "embedding",
// `detect_faces.rs`: "detecting"); `LibraryScan` stays at `''` for its
// entire run and `BulkRename` uses "renaming"/"undoing" — both ignored
// here, since they already have their own surface elsewhere
// (`ProblemsView.vue`, `RenameFormulaDialog.vue`).
type BackgroundKind = 'ai_analysis' | 'face_detection'
const PHASE_TO_KIND: Record<string, BackgroundKind> = {
  embedding: 'ai_analysis',
  detecting: 'face_detection'
}
interface BackgroundOp {
  operationId: string
  kind: BackgroundKind
  done: number
  total: number | null
}
const backgroundOps = ref<Map<string, BackgroundOp>>(new Map())
const backgroundOpList = computed(() => Array.from(backgroundOps.value.values()))
let live: LiveSocket | undefined

interface OperationProgressPayload {
  operation_id: string
  done: number
  total: number | null
  phase: string
}

onMounted(() => {
  live = startLiveEvents((msg) => {
    if (msg.type !== 'operation.progress') return
    const payload = msg.payload as OperationProgressPayload
    const kind = PHASE_TO_KIND[payload.phase]
    if (kind) {
      backgroundOps.value = new Map(backgroundOps.value).set(payload.operation_id, {
        operationId: payload.operation_id,
        kind,
        done: payload.done,
        total: payload.total
      })
      return
    }
    // Terminal phase ("done"/"cancelled"/"failed") or a different
    // operation type: if we knew about it, it leaves the panel.
    if (backgroundOps.value.has(payload.operation_id)) {
      const next = new Map(backgroundOps.value)
      next.delete(payload.operation_id)
      backgroundOps.value = next
    }
  })
})

onUnmounted(() => {
  live?.close()
})

const LIBRARY_ITEMS = [
  { to: '/folders', labelKey: 'folders.entry' },
  { to: '/map', labelKey: 'maps.entry' },
  { to: '/shares', labelKey: 'shares.entry' },
  { to: '/favorites', labelKey: 'favorites.entry' },
  { to: '/persons', labelKey: 'persons.entry' }
] as const

const MAINT_ITEMS = [
  { to: '/trash', labelKey: 'trash.entry' },
  { to: '/duplicates', labelKey: 'duplicates.entry' },
  { to: '/problems', labelKey: 'problems.title' }
] as const

const ADMIN_ITEMS = [
  { to: '/users', labelKey: 'users.entry' },
  { to: '/groups', labelKey: 'groups.entry' }
] as const

const IA_ITEMS = [
  { to: '/tags', labelKey: 'tags.entry', badge: false },
  { to: '/review', labelKey: 'review.entry', badge: true }
] as const
</script>

<template>
  <main class="p-3.5">
    <template v-if="backgroundOpList.length > 0">
      <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
        {{ t('nav.backgroundActivity') }}
      </p>
      <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
        <li
          v-for="op in backgroundOpList"
          :key="op.operationId"
          class="border-b border-border px-3.5 py-3 last:border-b-0"
        >
          <p class="text-[13px] font-semibold">
            {{
              op.total !== null
                ? t(`backgroundOps.${op.kind === 'ai_analysis' ? 'aiAnalysisKnown' : 'faceDetectionKnown'}`, {
                  done: op.done,
                  total: op.total
                })
                : t(`backgroundOps.${op.kind === 'ai_analysis' ? 'aiAnalysisUnknown' : 'faceDetectionUnknown'}`, {
                  done: op.done
                })
            }}
          </p>
          <div class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-border/40">
            <div
              class="h-full rounded-full bg-accent transition-[width]"
              :class="{ 'animate-pulse': op.total === null }"
              :style="{ width: op.total ? `${Math.min(100, (op.done / op.total) * 100)}%` : '30%' }"
            />
          </div>
        </li>
      </ul>
    </template>
    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.libraryGroup') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in LIBRARY_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </li>
    </ul>

    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.manutenzione') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in MAINT_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </li>
    </ul>

    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.ia') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in IA_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center justify-between gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          <span>{{ t(item.labelKey) }}</span>
          <span
            v-if="item.badge && shell.badges.revision > 0"
            class="min-w-[18px] rounded-full bg-danger px-1.5 text-center text-[11px] font-bold text-white"
          >
            {{ shell.badges.revision }}
          </span>
        </RouterLink>
      </li>
    </ul>

    <template v-if="session.user?.role === 'admin'">
      <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
        {{ t('nav.amministrazione') }}
      </p>
      <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
        <li
          v-for="item in ADMIN_ITEMS"
          :key="item.to"
        >
          <RouterLink
            :to="item.to"
            class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                   last:border-b-0 hover:bg-border/30"
          >
            {{ t(item.labelKey) }}
          </RouterLink>
        </li>
      </ul>
    </template>
  </main>
</template>

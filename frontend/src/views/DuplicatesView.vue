<script setup lang="ts">
// Fase 11 Task 13 (2/N) — documento funzionale §46 "Duplicati",
// verificato riga per riga (righe 6984-7155). Vista costruita da zero:
// non esisteva (`AppSidebar.vue`/`MoreView.vue` la dichiaravano
// esplicitamente non ancora fatta).
//
// **Deviazione reale dal documento**: ogni blocco gruppo del mockup
// porta un "motivo probabile" in linguaggio naturale (`"Stesso file
// importato due volte — import manuale e poi sync automatico..."`),
// ma è testo scritto a mano nei due gruppi di prova del mockup
// (`DUPLICATE_GROUPS`), non un campo che `DuplicateGroupView`
// (`crates/keeppix-api/src/routes/duplicates.rs`) restituisce — il
// backend non ha modo di indovinare *perché* due file coincidono.
// Il sottotitolo del gruppo qui è quindi solo la parte reale
// (`reclaimable_bytes`), senza premettere un motivo inventato.
//
// **Seconda deviazione, più sottile**: nel mockup la copia proposta
// come "da tenere" è sempre quella senza suffisso di copia — garantito
// dalla costruzione dei dati di prova, mai un algoritmo reale. Sul
// backend `DuplicateRepo::members` ordina per `a.filename` (ordine
// alfabetico puro, `crates/keeppix-db/src/assets.rs:562`): uno spazio
// prima di un suffisso "(1)" ordina **prima** di un punto prima
// dell'estensione, quindi l'alfabetico non garantisce affatto "il file
// senza suffisso per primo". Qui si propone comunque il primo elemento
// restituito (`members[0]`) come default — un punto di partenza
// ragionevole, non una detection reale del "file originale" che
// nessuna query supporta. L'utente può comunque scegliere qualunque
// copia con un click, come da documento.
//
// Il resto è fedele: "Risolvi gruppo"/"Ignora" restano senza conferma
// propria (il dialog di eliminazione a 3 opzioni, §9, resta l'unica
// conferma) — ma qui, a differenza del mockup (nota per l'architetto,
// §46.9: "risolvere un gruppo non applica davvero" la modalità
// scelta), la scelta è propagata per davvero via
// `resolveDuplicateGroup`, che applica `disk_action` a ogni membro del
// gruppo tranne quello tenuto in un'unica chiamata reale.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { fetchDuplicateMembers, fetchDuplicates, resolveDuplicateGroup, type DuplicateGroup } from '@/api/library'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import DeleteDialog, { type DeleteChoice } from '@/components/ui/DeleteDialog.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import { classifyError } from '@/errors/classify'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const DISK_ACTION: Record<DeleteChoice, 'kept' | 'moved_to_trash' | 'purged'> = {
  index: 'kept',
  trash: 'moved_to_trash',
  disk: 'purged'
}

const { t, locale } = useI18n()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

const groups = ref<DuplicateGroup[]>([])
const membersByHash = ref<Record<string, TimelineAsset[]>>({})
const keepByHash = ref<Record<string, string>>({})
const ignoredHashes = ref<Set<string>>(new Set())
const loaded = ref(false)
const loadError = ref<unknown>(null)
const resolvingHash = ref<string | null>(null)
const resolveDialogOpen = ref(false)
const busyHashes = ref<Set<string>>(new Set())

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

const visibleGroups = computed(() => groups.value.filter((g) => !ignoredHashes.value.has(g.content_hash)))

async function load() {
  loadError.value = null
  loaded.value = false
  try {
    const list = await fetchDuplicates()
    groups.value = list
    const pairs = await Promise.all(
      list.map(async (g) => [g.content_hash, await fetchDuplicateMembers(g.content_hash).catch(() => [])] as const)
    )
    membersByHash.value = Object.fromEntries(pairs)
    const keeps: Record<string, string> = {}
    for (const [hash, members] of pairs) {
      const first = members[0]
      if (first) keeps[hash] = first.id
    }
    keepByHash.value = keeps
    loaded.value = true
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = error
  }
}

onMounted(load)

function formatMB(bytes: number): string {
  return new Intl.NumberFormat(locale.value, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(
    bytes / (1024 * 1024)
  )
}

const pageSubtitle = computed(() => {
  const n = visibleGroups.value.length
  const m = visibleGroups.value.reduce((sum, g) => sum + g.count, 0)
  const mb = formatMB(visibleGroups.value.reduce((sum, g) => sum + g.reclaimable_bytes, 0))
  return t('duplicates.subtitle', { n, m, mb }, { plural: n })
})

function groupTitle(group: DuplicateGroup): string {
  return t('duplicates.groupTitle', { n: group.count })
}

function groupSubtitle(group: DuplicateGroup): string {
  return t('duplicates.groupSubtitle', { mb: formatMB(group.reclaimable_bytes) })
}

function thumbnailUrl(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

function placeholderUrl(asset: TimelineAsset): string | undefined {
  return asset.thumbhash ? (thumbhashToDataURL(asset.thumbhash) ?? undefined) : undefined
}

function keep(hash: string, assetId: string) {
  keepByHash.value = { ...keepByHash.value, [hash]: assetId }
}

function openResolve(hash: string) {
  resolvingHash.value = hash
  resolveDialogOpen.value = true
}

async function confirmResolve(choice: DeleteChoice) {
  const hash = resolvingHash.value
  if (!hash) return
  const keepId = keepByHash.value[hash]
  const members = membersByHash.value[hash] ?? []
  if (!keepId || busyHashes.value.has(hash)) return
  busyHashes.value = new Set(busyHashes.value).add(hash)
  try {
    const { resolved } = await resolveDuplicateGroup(hash, keepId, DISK_ACTION[choice])
    const keptAsset = members.find((m) => m.id === keepId)
    toast.show(t('duplicates.resolved', { n: resolved, name: keptAsset?.filename ?? '' }, { plural: resolved }))
    groups.value = groups.value.filter((g) => g.content_hash !== hash)
  } catch {
    toast.showError(t('duplicates.actionError'))
  } finally {
    const next = new Set(busyHashes.value)
    next.delete(hash)
    busyHashes.value = next
  }
}

function ignore(hash: string) {
  ignoredHashes.value = new Set(ignoredHashes.value).add(hash)
  toast.show(t('duplicates.ignored'))
}

const resolveDialogTitle = computed(() => {
  const hash = resolvingHash.value
  if (!hash) return ''
  const count = groups.value.find((g) => g.content_hash === hash)?.count ?? 0
  return t('duplicates.resolveTitle', { n: Math.max(count - 1, 0) }, { plural: Math.max(count - 1, 0) })
})
</script>

<template>
  <main class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="load"
    />
    <div
      v-else-if="loaded && visibleGroups.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('duplicates.emptyTitle') }}
      </p>
      <p class="max-w-[380px] text-sm text-content-muted">
        {{ t('duplicates.emptySubtitle') }}
      </p>
    </div>
    <template v-else>
      <div class="border-b border-border px-4 py-3">
        <p class="text-[15px] font-bold">
          {{ t('duplicates.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ pageSubtitle }}
        </p>
      </div>

      <div class="flex flex-col gap-2.5 p-4">
        <div
          v-for="group in visibleGroups"
          :key="group.content_hash"
          class="flex items-start gap-3 rounded-[10px] border border-border p-3.5"
        >
          <div class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-full bg-accent-tint text-accent">
            ⧉
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-[13.5px] font-bold">
              {{ groupTitle(group) }}
            </p>
            <p class="text-[12px] text-content-muted">
              {{ groupSubtitle(group) }}
            </p>

            <div class="mt-2.5 flex gap-[5px] overflow-x-auto pb-1">
              <div
                v-for="asset in membersByHash[group.content_hash] ?? []"
                :key="asset.id"
                class="w-[70px] shrink-0 text-center"
              >
                <button
                  type="button"
                  role="button"
                  :aria-pressed="keepByHash[group.content_hash] === asset.id"
                  :aria-label="
                    keepByHash[group.content_hash] === asset.id
                      ? t('duplicates.keepLabel', { name: asset.filename })
                      : t('duplicates.discardLabel', {
                        name: membersByHash[group.content_hash]?.find((m) => m.id === keepByHash[group.content_hash])
                          ?.filename
                      })
                  "
                  class="relative h-[70px] w-[70px] overflow-hidden rounded-lg outline outline-2 outline-offset-1 transition-[opacity,outline-color]"
                  :style="{ transitionDuration: 'var(--duration-fast)' }"
                  :class="
                    keepByHash[group.content_hash] === asset.id
                      ? 'opacity-100 outline-accent'
                      : 'opacity-55 outline-transparent hover:opacity-80'
                  "
                  @click="keep(group.content_hash, asset.id)"
                >
                  <img
                    v-if="thumbnailUrl(asset)"
                    :src="thumbnailUrl(asset)"
                    alt=""
                    class="h-full w-full object-cover"
                  >
                  <img
                    v-else-if="placeholderUrl(asset)"
                    :src="placeholderUrl(asset)"
                    alt=""
                    class="h-full w-full object-cover"
                  >
                  <div
                    v-else
                    class="h-full w-full bg-border/30"
                  />
                  <span
                    v-if="keepByHash[group.content_hash] === asset.id"
                    class="absolute top-1 right-1 flex h-4 w-4 items-center justify-center rounded-full bg-accent text-[10px] text-white"
                  >
                    ✓
                  </span>
                </button>
                <p
                  class="mt-1 truncate text-[10.5px]"
                  :class="keepByHash[group.content_hash] === asset.id ? 'font-semibold text-accent' : 'text-content-muted'"
                >
                  {{ asset.filename }}
                </p>
                <p class="text-[10px] text-content-muted">
                  {{ formatMB(asset.size_bytes) }} MB
                </p>
              </div>
            </div>

            <div class="mt-2.5 flex gap-2">
              <button
                type="button"
                class="rounded-lg border border-border-strong bg-surface-elevated px-2.5 py-1.5 text-[12px] font-semibold hover:bg-border/20 disabled:opacity-50"
                :disabled="busyHashes.has(group.content_hash)"
                @click="openResolve(group.content_hash)"
              >
                {{ t('duplicates.resolve') }}
              </button>
              <button
                type="button"
                class="rounded-lg px-2.5 py-1.5 text-[12px] font-semibold text-content-muted hover:bg-border/20"
                @click="ignore(group.content_hash)"
              >
                {{ t('duplicates.ignore') }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </template>

    <DeleteDialog
      v-model:open="resolveDialogOpen"
      :title="resolveDialogTitle"
      @choose="confirmResolve"
    />
  </main>
</template>

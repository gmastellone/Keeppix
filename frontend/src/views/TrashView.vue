<script setup lang="ts">
// Fase 11 Task 13 (1/N) — documento funzionale §45 "Cestino", verificato
// riga per riga (righe 6841-6981). Riscrittura completa: la vista
// precedente aveva un bug reale, non solo un buco di funzionalità — vedi
// il commento in `api/trash.ts` (`TrashedItem`) per il dettaglio.
//
// **Due deviazioni reali dal documento**, entrambe per capacità reale
// del backend diversa dal mockup, non per scelta stilistica:
//
// 1. **Miniatura vera, non un gradiente finto**: il mockup mostra "il
//    gradiente della foto come miniatura" (stesso trucco delle
//    copertine album, §41) perché la sua base dati (`STATE.trash`) non
//    porta un'immagine reale. Qui gli elementi in cestino SONO foto
//    vere del catalogo (`status='trashed'`, non cancellate dalla
//    tabella `assets` finché non si sceglie "Elimina definitivamente"):
//    `GET /assets/{id}` le trova ancora, con `content_hash`/`thumbhash`
//    validi. Mostrare un gradiente al posto della vera foto sarebbe un
//    passo indietro reale — senza nome file (mai mostrato, per
//    documento) sarebbe impossibile riconoscere cosa si sta per
//    ripristinare o eliminare per sempre. Pattern N+1 già usato altrove
//    in questa fase (Task 9/11/12): un `fetchAsset` per elemento, pochi
//    elementi attesi in un cestino.
// 2. **"<N> giorni rimanenti" è il vero conto alla rovescia**, non
//    `20 + hash(id)%10` — il backend calcola `days_remaining` da
//    `deleted_at` reale + 30 giorni (vedi `api/trash.ts`), la scadenza
//    "annunciata ma non implementata" del mockup è implementata per
//    davvero qui.
//
// **Fedele al documento nonostante siano azioni reali e permanenti**:
// "Svuota cestino" e "Elimina definitivamente" restano senza dialog di
// conferma, senza toast di successo, senza possibilità di annullare —
// è il comportamento esplicitamente voluto dal documento (§45.3, righe
// 6880/6882: "senza chiedere conferma... senza toast... senza
// annullamento"), non un debito di questa unità. Un errore di rete
// resta comunque segnalato (toast), perché sul backend reale queste
// chiamate possono davvero fallire (403/409/500) — il mockup non lo
// prevede solo perché la sua base dati non può fallire.
//
// Accessibilità da tastiera corretta rispetto al mockup (§45.5, "la
// vista meno accessibile del blocco" — un difetto dichiarato, non una
// scelta): i due pulsantini per riquadro sono `&lt;button&gt;` reali,
// rivelati anche da `:focus-within` oltre che da `:hover`, coerente con
// il resto dell'app (SP-1).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { deleteAsset } from '@/api/culling'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import { fetchAsset, type TimelineAsset } from '@/api/timeline'
import { emptyTrash, fetchTrash, restoreAsset, type TrashedItem } from '@/api/trash'
import ErrorState from '@/components/ui/ErrorState.vue'
import { classifyError } from '@/errors/classify'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

const items = ref<TrashedItem[]>([])
const assetsById = ref<Record<string, TimelineAsset>>({})
const loaded = ref(false)
const loadError = ref<unknown>(null)
const emptying = ref(false)
const pending = ref<Set<string>>(new Set())

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

async function load() {
  loadError.value = null
  loaded.value = false
  try {
    const collected: TrashedItem[] = []
    let cursor: string | undefined
    do {
      const page = await fetchTrash(cursor)
      collected.push(...page.items)
      cursor = page.next_cursor
    } while (cursor)
    items.value = collected
    const pairs = await Promise.all(
      collected.map(async (item) => [item.asset_id, await fetchAsset(item.asset_id).catch(() => null)] as const)
    )
    assetsById.value = Object.fromEntries(pairs.filter((pair): pair is [string, TimelineAsset] => pair[1] !== null))
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

function thumbnailUrl(item: TrashedItem): string | undefined {
  const asset = assetsById.value[item.asset_id]
  return asset?.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

function placeholderUrl(item: TrashedItem): string | undefined {
  const hash = assetsById.value[item.asset_id]?.thumbhash
  return hash ? (thumbhashToDataURL(hash) ?? undefined) : undefined
}

async function restore(item: TrashedItem) {
  if (pending.value.has(item.asset_id)) return
  pending.value = new Set(pending.value).add(item.asset_id)
  try {
    await restoreAsset(item.asset_id)
    items.value = items.value.filter((i) => i.id !== item.id)
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    const next = new Set(pending.value)
    next.delete(item.asset_id)
    pending.value = next
  }
}

async function purge(item: TrashedItem) {
  if (pending.value.has(item.asset_id)) return
  pending.value = new Set(pending.value).add(item.asset_id)
  try {
    await deleteAsset(item.asset_id, 'purged')
    items.value = items.value.filter((i) => i.id !== item.id)
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    const next = new Set(pending.value)
    next.delete(item.asset_id)
    pending.value = next
  }
}

async function emptyAll() {
  if (emptying.value) return
  emptying.value = true
  try {
    await emptyTrash()
    items.value = []
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    emptying.value = false
  }
}
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
      v-else-if="loaded && items.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('trash.emptyTitle') }}
      </p>
      <p class="max-w-[380px] text-sm text-content-muted">
        {{ t('trash.emptySubtitle') }}
      </p>
    </div>
    <template v-else>
      <div class="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <p class="text-[15px] font-bold">
            {{ t('trash.title') }}
          </p>
          <p class="text-sm text-content-muted">
            {{ t('trash.subtitle', { n: items.length }, { plural: items.length }) }}
          </p>
        </div>
        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10 disabled:opacity-50"
          :disabled="emptying"
          @click="emptyAll"
        >
          {{ t('trash.emptyAll') }}
        </button>
      </div>

      <div
        class="grid gap-3 p-4"
        style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr))"
      >
        <div
          v-for="item in items"
          :key="item.id"
          class="group relative aspect-square overflow-hidden rounded-[5px] border border-border"
        >
          <img
            v-if="thumbnailUrl(item)"
            :src="thumbnailUrl(item)"
            :alt="''"
            class="absolute inset-0 h-full w-full object-cover"
            loading="lazy"
          >
          <img
            v-else-if="placeholderUrl(item)"
            :src="placeholderUrl(item)"
            :alt="''"
            class="absolute inset-0 h-full w-full object-cover"
          >
          <div
            v-else
            class="absolute inset-0 bg-border/30"
          />

          <span
            class="absolute bottom-[2.5px] left-[2.5px] right-[2.5px] rounded-md bg-black/60 py-0.5 text-center text-[9.5px] font-bold text-white"
          >
            {{ t('trash.daysRemaining', { n: item.days_remaining }, { plural: item.days_remaining }) }}
          </span>

          <div
            class="absolute inset-0 flex items-center justify-center gap-1.5 bg-black/45 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
          >
            <button
              type="button"
              :aria-label="t('trash.restore')"
              :title="t('trash.restore')"
              :disabled="pending.has(item.asset_id)"
              class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-content disabled:opacity-50"
              @click.stop="restore(item)"
            >
              ↻
            </button>
            <button
              type="button"
              :aria-label="t('trash.purge')"
              :title="t('trash.purge')"
              :disabled="pending.has(item.asset_id)"
              class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-danger disabled:opacity-50"
              @click.stop="purge(item)"
            >
              🗑
            </button>
          </div>
        </div>
      </div>
    </template>
  </main>
</template>

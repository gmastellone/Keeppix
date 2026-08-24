<script setup lang="ts">
// Fase 11 Task 17 (2/N) — dialog "Scegli la cartella radice di culling"
// (documento funzionale §17). Non costruito sopra `Dialog.vue`: quel
// componente risolve *a favore* di SP-5 (trappola del focus, scrim che
// chiude) — qui la spec vuole l'esatto contrario, tre deviazioni
// esplicite (§17.4-5): click sullo scrim non chiude, nessuna trappola
// del focus, nessun elemento riceve il fuoco all'apertura (il fuoco
// torna al trigger alla chiusura, quello sì). `DialogRoot :modal="false"`
// è l'unica combinazione di reka-ui che porta `trap-focus` a `false` —
// la variante modale lo forza sempre a `true` mentre il dialog è aperto
// (`DialogContentModal`, non scavalcabile via prop). Lo scrim è un `div`
// decorativo senza gestore, non `DialogOverlay`: quel componente non si
// monta affatto quando `modal="false"`.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { DialogContent, DialogDescription, DialogPortal, DialogRoot, DialogTitle } from 'reka-ui'

import { fetchChildren, type FolderView } from '@/api/folders'

const { t } = useI18n()

const open = defineModel<boolean>('open', { required: true })
/** Radice-a-foglia: il primo elemento è sempre la radice della libreria
 * (mai vuoto — il chiamante lo garantisce). Se la cartella oggi
 * configurata non esiste più nell'albero, il chiamante passa `[radice]`
 * — "riparte dalla radice" (§17.2). */
const props = defineProps<{ initialPath: FolderView[] }>()
const emit = defineEmits<{ confirm: [folderId: string] }>()

const crumbs = ref<FolderView[]>([])
const children = ref<FolderView[]>([])
const loading = ref(false)

function preventDefault(event: Event) {
  event.preventDefault()
}

async function loadChildrenOf(folder: FolderView) {
  loading.value = true
  try {
    const result = await fetchChildren(folder.id)
    children.value = result.folders
  } finally {
    loading.value = false
  }
}

watch(open, (isOpen) => {
  if (!isOpen) return
  crumbs.value = [...props.initialPath]
  void loadChildrenOf(crumbs.value[crumbs.value.length - 1])
})

function enter(folder: FolderView) {
  crumbs.value = [...crumbs.value, folder]
  void loadChildrenOf(folder)
}

function goToCrumb(index: number) {
  if (index >= crumbs.value.length - 1) return
  crumbs.value = crumbs.value.slice(0, index + 1)
  void loadChildrenOf(crumbs.value[crumbs.value.length - 1])
}

function confirm() {
  const current = crumbs.value[crumbs.value.length - 1]
  open.value = false
  emit('confirm', current.id)
}
</script>

<template>
  <DialogRoot
    v-model:open="open"
    :modal="false"
  >
    <DialogPortal>
      <div class="fixed inset-0 z-40 bg-black/50" />
      <DialogContent
        aria-modal="true"
        class="fixed top-1/2 left-1/2 z-50 w-[420px] max-w-[86%] -translate-x-1/2 -translate-y-1/2
               rounded-xl bg-[var(--color-surface-elevated)] p-6 shadow-xl focus:outline-none"
        @open-auto-focus="preventDefault"
        @pointer-down-outside="preventDefault"
        @interact-outside="preventDefault"
      >
        <DialogTitle class="text-lg font-semibold text-[var(--color-content)]">
          {{ t('cullingRootPicker.title') }}
        </DialogTitle>
        <DialogDescription class="mt-1 text-sm text-[var(--color-content-muted)]">
          {{ t('cullingRootPicker.subtitle') }}
        </DialogDescription>

        <nav
          class="mt-4 flex flex-wrap items-center gap-1 text-xs text-content-muted"
          :aria-label="t('cullingRootPicker.title')"
        >
          <template
            v-for="(crumb, index) in crumbs"
            :key="crumb.id"
          >
            <span v-if="index > 0">/</span>
            <button
              type="button"
              tabindex="-1"
              class="hover:text-content hover:underline"
              @click="goToCrumb(index)"
            >
              {{ index === 0 ? t('cullingRootPicker.rootCrumb') : crumb.name }}
            </button>
          </template>
        </nav>

        <ul class="mt-2 max-h-[220px] overflow-y-auto rounded-lg border border-border">
          <li
            v-if="!loading && children.length === 0"
            class="px-3 py-2 text-[13px] text-content-muted"
          >
            {{ t('cullingRootPicker.empty') }}
          </li>
          <li
            v-for="folder in children"
            :key="folder.id"
          >
            <button
              type="button"
              class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-[13px]
                     transition-colors hover:bg-border/20"
              @click="enter(folder)"
            >
              <span class="truncate">{{ folder.name }}</span>
              <svg
                viewBox="0 0 20 20"
                class="h-3 w-3 shrink-0"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M7 4l6 6-6 6"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          </li>
        </ul>

        <div class="mt-4 flex gap-2">
          <button
            type="button"
            class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white transition-[filter]
                   hover:brightness-105"
            @click="confirm"
          >
            {{ t('cullingRootPicker.confirm') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-transparent bg-transparent px-3.5 py-2 text-[13px]
                   font-semibold text-content hover:bg-border/40"
            @click="open = false"
          >
            {{ t('cullingRootPicker.cancel') }}
          </button>
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<script setup lang="ts">
// Fase 11 Task 8 (2/N…8/N) — documento funzionale §18 ("Lightbox —
// struttura e barra superiore"), §19 ("Pannello informazioni") e §20
// ("Menu 'altre azioni' ⋯"). La 2/N ha riscritto il segnaposto precedente
// (151 righe) con barra superiore, stage con frecce, filmino e menu ⋯. La
// 3/N ha aggiunto titolo modificabile, valutazione a stelle, sezione
// SCATTO. La 4/N ha completato la sezione POSIZIONE. La 5/N ha aggiunto
// la sezione PERSONE e i riquadri volto. La 6/N ha aggiunto la sezione
// TAG. La 7/N ha chiuso §19 con ALBUM e AZIONI. La 8/N (questa) ha
// costruito il commutatore RAW/JPEG (§19.2 riga 5) — **non** come nel
// mockup (dove "l'unico effetto osservabile è quale chip è evidenziata",
// l'immagine mostrata non cambia mai): qui la selezione cambia davvero
// cosa viene mostrato sullo stage e cosa scarica "Scarica originale", il
// punto che il documento stesso indica come "il backend dovrà fare
// qualcosa di vero: scegliere quale dei due file della pila viene
// decodificato, mostrato e scaricato" — decodifica e mostra funzionano
// già gratis via `/media/preview/{hash}` (i RAW hanno già un'anteprima
// derivata, stesso motivo per cui le miniature RAW funzionano ovunque
// nell'app), serviva solo instradare la scelta dell'utente al membro
// giusto dello stack (`GET /assets/{id}/stack`, Fase 10, primo consumo
// frontend). Con la 8/N §19 è costruito per intero, salvo il debito
// dichiarato sotto. La 9/N corregge un bug reale trovato rileggendo
// §19.8: "il pannello esiste solo dentro il lightbox... **ed è forzato
// aperto a ogni `openLightbox()`** (e all'apertura dal culling)" — dalla
// 2/N in poi il pannello partiva **chiuso**, mai notato perché ogni test
// di questo file lo apriva esplicitamente con `i` prima di verificare
// qualunque contenuto (mascherando quindi il difetto invece di
// scoprirlo). Corretto: `info` parte `true`, `loadPanelData()` scatta da
// `onMounted`, non solo dal primo `i`/click sull'icona. La 10/N (questa)
// aggiunge il prop `isCulling` (§21, "Differenze fra lightbox aperto da
// libreria e lightbox aperto da un lotto di culling"): nasconde PERSONE/
// TAG/ALBUM e le azioni "Aggiungi ad album"/"Elimina…" (pannello e menu
// ⋯) quando la foto viene da un lotto ancora non organizzato — tutto il
// resto (titolo, stelle, RAW/JPEG, SCATTO, POSIZIONE, filmino, frecce,
// barra superiore, Scarica/Ruota/Rinomina) resta identico, come da
// tabella del documento. `CullingView.vue` è il primo (e unico)
// chiamante con `isCulling`; i quattro chiamanti libreria (Timeline,
// Preferiti, Cerca, Mappa) restano `isCulling` di default (`false`).
//
// **Deviazione deliberata dal mockup, non un debito**: il documento
// descrive tre stati di chip per un tag confermato — "applicato dall'IA,
// mai revisionato" (opacità ridotta, marcatore "IA", click-per-
// confermare) contro "confermato da un umano" (pieno, nessun marcatore).
// Nel backend reale questa distinzione **non esiste**: `AssetTagRepo::
// decide` (`confirm`/`reject`) transita solo righe `state='proposed'` —
// una riga `state='confirmed'` è per costruzione già stata decisa (da
// `confirm()`, che richiede un utente autenticato, o da un'assegnazione
// manuale), non importa se il suo `source` originario era `'ai'` o
// `'user'`. Riprodurre il marcatore "IA, clicca per confermare" su un
// tag già confermato sarebbe un pulsante che promette un'azione (una
// seconda "conferma") che non ha alcun effetto reale — `decide()` è
// idempotente e non fa nulla se lo stato è già quello richiesto. Ogni
// tag confermato ha quindi **un solo aspetto**, indipendente da `source`;
// la distinzione a tre vie del mockup collassa correttamente nelle due
// sezioni reali del backend: confermato (fatto) e proposto (da
// decidere).
//
// **Debito dichiarato, verificato e non taciuto**:
// - "Condividi" (§18.3 riga 3, Task 11 1/N): apre `ShareSelectionDialog
//   .vue` per questo singolo asset — stesso meccanismo (album
//   auto-generato) già usato dalla barra di selezione, vedi lì per il
//   perché.
// - "Ruota" resta un toast (nessuna pipeline di rotazione reale esiste
//   ancora — dichiarato nel Task 8 1/N: `orientation` è scrivibile ma
//   mai consumato da `keeppix-media`).
// - Il link verso la cartella/il lotto di provenienza nella riga
//   data/ora (§19.2 riga 2) è omesso: non esiste una rotta per risolvere
//   il nome di una cartella dal solo `folder_id` (`GET /folders/{id}`
//   non esiste, solo `tree`/`{id}/children`) — costruirla per una sola
//   riga di sottotitolo non è nello scopo di questa unità.
// - "Vai alla persona" (§19.3/§38, prima voce del menu del chip): era
//   omesso qui (debito dichiarato: "nessuna vista Persone esiste
//   ancora"), costruito nel Task 16 (3/N) ora che `/persons/:id` esiste
//   — chiude il menu, chiude il lightbox (`emit('close')`), naviga.
// - **Il menu sul riquadro del volto resta un `Popover` ancorato, non un
//   dialog modale** (Task 16 3/N, verificato contro §38/§40 SP-14: "il
//   menu sul riquadro del volto **non** usa questo pattern: è un dialog
//   modale, non un menu a comparsa ancorato" — deviazione reale, non
//   corretta qui: riscrivere il contenitore da `Popover` a `Dialog`
//   toccherebbe anche l'hover/focus dei riquadri sulla foto (§38.4-5,
//   200 ms di tolleranza) per un guadagno di fedeltà strutturale, non di
//   contenuto — le tre opzioni con titolo+descrizione (§38.2) sono ora
//   comunque tutte presenti e reali).
// - Il chip "+ aggiungi" delle persone (§19.2 riga 13, ultimo chip) non
//   è costruito: aggiungere una persona a mano crea un volto **senza**
//   rilevamento dietro (`box:null` nel mockup), ma `Face.bbox` nel
//   dominio reale (`crates/keeppix-domain/src/face.rs`) non è opzionale
//   — un vero buco di modello, non di frontend, già rimandato al Task A
//   (Volti: YuNet+SFace) fin dal Task 8 (1/N): quella è la sede naturale
//   per rivedere una volta sola il modello `Face`.
//
// **Corretto qui, non solo aggiunto**: il click sullo sfondo nero
// *non* deve chiudere il lightbox (§18.4, esplicito — a differenza
// dello scrim dei dialog modali, SP-5) — la versione precedente aveva
// `@click.self="emit('close')"` sul contenitore radice, un
// comportamento mai documentato per questa vista. Rimosso.
//
// **Bug reale trovato e corretto in questa unità (7/N), presente fin dal
// Task 8 (2/N)**: `Esc` chiudeva **anche il lightbox sotto**, non solo il
// dialog aperto, ogni volta che uno dei sei dialog del pannello (elimina,
// album, rinomina, posizione, persona, tag) era aperto — non solo il
// menu ⋯, l'unico caso gestito finora. La gestione di Esc di reka-ui gira
// su `DismissableLayer`, un meccanismo interno che non coordina in alcun
// modo con l'`window.addEventListener('keydown', onKey)` scritto a mano
// qui: nessuno dei sei dialog era mai stato insegnato a `onKey`. Scoperto
// scrivendo i test della sezione ALBUM (§19.2 riga 18), mai prima
// esercitato da nessun test di questo file. Corretto con `dialogRefs`,
// controllato prima di ricadere sull'`emit('close')` del lightbox.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { deleteAsset, fetchFlags, setFlags, unvotedFlags } from '@/api/culling'
import type { AssetFlags, DiskAction } from '@/api/culling'
import { fetchAlbumsForAsset, type AlbumBadge } from '@/api/albums'
import { assignFace, fetchFacesForAsset, rejectFace, type Face } from '@/api/faces'
import { fetchMetadata, patchMetadata, type AssetMetadata } from '@/api/metadata'
import { originalSrc, previewSrc as mediaPreviewSrc, thumbSrc as mediaThumbSrc } from '@/api/media'
import { fetchStack, type StackMember } from '@/api/stacks'
import {
  confirmTagProposal,
  fetchTags,
  fetchTagsForAsset,
  rejectTagProposal,
  removeConfirmedTag,
  type AssetTagDetail,
  type Tag
} from '@/api/tags'
import { fetchAsset, type TimelineAsset } from '@/api/timeline'
import AlbumPickerDialog from '@/components/AlbumPickerDialog.vue'
import PersonPickerDialog from '@/components/PersonPickerDialog.vue'
import PlacePickerDialog from '@/components/PlacePickerDialog.vue'
import RatingStars from '@/components/RatingStars.vue'
import RenameFormulaDialog from '@/components/RenameFormulaDialog.vue'
import ShareSelectionDialog from '@/components/ShareSelectionDialog.vue'
import TagPickerDialog from '@/components/TagPickerDialog.vue'
import DeleteDialog, { type DeleteChoice } from '@/components/ui/DeleteDialog.vue'
import Popover from '@/components/ui/Popover.vue'
import MapClusterLayer from '@/components/MapClusterLayer.vue'
import { useMapsStore } from '@/stores/maps'
import { useToastStore } from '@/stores/toast'

const props = withDefaults(
  defineProps<{
    asset: TimelineAsset
    /** L'insieme di navigazione (frecce + filmino), nell'ordine di
     * visualizzazione — §18.2/§18.8: "tutte le foto della stessa
     * cartella e dello stesso mese" per la libreria, già calcolato dal
     * chiamante (ogni vista sa qual è il proprio "vicinato": `loadedAssets`
     * per Timeline, `filteredAssets` per Preferiti/Cerca). Vuoto di
     * default: nessuna freccia, nessun filmino — il popover della mappa
     * non ha un concetto di vicinato e continua a funzionare senza
     * modifiche. */
    neighbors?: TimelineAsset[]
    isFavorite: boolean
    /** §21: la foto viene da un lotto di culling, non ancora organizzata
     * nella libreria — niente cartella/mese/tag/album/volti. Nasconde le
     * sezioni PERSONE/TAG/ALBUM e le azioni "Aggiungi ad album"/
     * "Elimina…" (pannello e menu ⋯); tutto il resto (titolo, stelle,
     * RAW/JPEG, SCATTO, POSIZIONE, filmino, frecce, barra superiore,
     * Scarica/Ruota/Rinomina) resta identico. `false` di default: gli
     * altri quattro chiamanti (Timeline/Preferiti/Cerca/Mappa) sono
     * sempre contesto libreria. */
    isCulling?: boolean
  }>(),
  { neighbors: () => [], isCulling: false }
)
const emit = defineEmits<{
  close: []
  /** Sostituisce i due emit separati `prev`/`next` del segnaposto
   * precedente: frecce, filmino e tastiera risolvono già l'asset di
   * destinazione da `neighbors`, il chiamante non deve più rifare la
   * stessa ricerca (`viewingNeighbour`) che il vecchio contratto gli
   * imponeva. */
  step: [asset: TimelineAsset]
  'open-asset': [id: string]
  'toggle-favorite': []
}>()
const { t, locale } = useI18n()
const maps = useMapsStore()
const toast = useToastStore()
const router = useRouter()

/** §19.8: "forzato aperto a ogni `openLightbox()` (e all'apertura dal
 * culling)" — non chiuso di default come nella versione precedente di
 * questo componente. `I`/l'icona restano il modo per chiuderlo (e
 * riaprirlo), invariati. */
const info = ref(true)
const moreOpen = ref(false)
const albumDialogOpen = ref(false)
const shareDialogOpen = ref(false)
const renameDialogOpen = ref(false)
const deleteDialogOpen = ref(false)
const positionDialogOpen = ref(false)
const metadata = ref<AssetMetadata>()
/** §19.2 sezione "SCATTO": `full_exif` non arriva mai col prop `asset`
 * (le griglie che passano l'asset al lightbox usano `/timeline`/`/search`,
 * che non lo calcolano) — solo `GET /assets/{id}` lo porta. */
const detail = ref<TimelineAsset>()
const flags = ref<AssetFlags>()
const placeName = ref<string | null>(null)
const titleDraft = ref('')
/** §18.2/§19.2: volti rilevati con riquadro (`bbox`), separati da
 * `asset.faces` (`AssetFaceBadge[]`, solo `person_id`/`person_name`, già
 * disponibile dal prop senza fetch) — serve a mappare ogni chip persona
 * al/i volto/i corrispondente/i sull'immagine. */
const faces = ref<Face[]>([])
const personDialogOpen = ref(false)
const assetTags = ref<AssetTagDetail[]>([])
/** Solo `kind === 'category'` da `GET /tags` (elenco unico tag+categorie,
 * §19.2 righe 14-17): serve per il nome di ogni gruppo — `AssetTagDetail.
 * category_id` porta solo l'id. */
const categories = ref<Tag[]>([])
const tagDialogOpen = ref(false)
/** §19.2 riga 18: elenco di sola lettura degli album di cui la foto è
 * membro (manuali e dinamici indistinti, `AlbumRepo::for_asset`) — "+
 * aggiungi" riusa `albumDialogOpen`/`AlbumPickerDialog`, già cablato dal
 * Task 8 2/N per il menu ⋯: stesso dialog, due punti d'ingresso. */
const assetAlbums = ref<AlbumBadge[]>([])
/** Il volto che "Correggi persona…" sta per riassegnare — impostato
 * all'apertura del selettore, letto quando l'utente sceglie una persona. */
const correctingFaceId = ref<string | null>(null)
const openFaceMenuPersonId = ref<string | null>(null)
const hoveredPersonId = ref<string | null>(null)
let hideBoxesTimer: ReturnType<typeof setTimeout> | undefined
let panelRequestSequence = 0

function previewSrc(asset: TimelineAsset): string {
  return asset.content_hash
    ? mediaPreviewSrc(asset.content_hash)
    : `/media/original/${asset.id}`
}

/** §19.2 riga 5, commutatore RAW/JPEG: a differenza del mockup ("l'unico
 * effetto osservabile è quale chip è evidenziata... non cambia l'immagine
 * mostrata"), qui la selezione **cambia davvero cosa viene mostrato e
 * scaricato** — il documento stesso lo indica come uno dei punti in cui
 * "il backend dovrà fare qualcosa di vero: scegliere quale dei due file
 * della pila viene decodificato, mostrato e scaricato". `stackMembers`
 * arriva solo per un asset `raw_kind` `'raw'`/`'raw+jpeg'` (mai per un
 * JPEG semplice, che non ha uno stack). */
const stackMembers = ref<StackMember[]>([])
const selectedStackMemberId = ref<string | null>(null)
const rawMember = computed(() => stackMembers.value.find((m) => m.raw_kind === 'raw'))
const jpegMember = computed(() => stackMembers.value.find((m) => m.raw_kind === 'jpeg'))
const selectedStackMember = computed(() =>
  stackMembers.value.find((m) => m.id === selectedStackMemberId.value)
)

const src = computed(() => previewSrc(selectedStackMember.value ?? props.asset))
const downloadTarget = computed(() => selectedStackMember.value ?? props.asset)

const currentIndex = computed(() => props.neighbors.findIndex((n) => n.id === props.asset.id))
const prevAsset = computed(() =>
  currentIndex.value > 0 ? props.neighbors[currentIndex.value - 1] : undefined
)
const nextAsset = computed(() =>
  currentIndex.value >= 0 && currentIndex.value < props.neighbors.length - 1
    ? props.neighbors[currentIndex.value + 1]
    : undefined
)
const prevSrc = computed(() => (prevAsset.value ? previewSrc(prevAsset.value) : undefined))
const nextSrc = computed(() => (nextAsset.value ? previewSrc(nextAsset.value) : undefined))

function stepTo(target: TimelineAsset | undefined) {
  if (target) emit('step', target)
}

/** §18.5: `Esc` a due livelli — un menu ⋯ aperto ne assorbe la prima
 * pressione. Controllato qui, non lasciato al layering di reka-ui (il
 * lightbox stesso non è un `DialogRoot`/`PopoverRoot`, solo il menu ⋯
 * lo è): un singolo `keydown` globale deve sapere qual è il primo
 * livello da chiudere prima di arrivare al secondo. */
/** I sei dialog aperti dal pannello/menu (elimina, album, rinomina,
 * posizione, persona, tag) sono tutti componenti reka-ui reali con la
 * propria gestione di Esc — ma quella gira su `DismissableLayer`, un
 * meccanismo interno alla libreria che non coordina in alcun modo con un
 * `window.addEventListener('keydown', ...)` scritto a mano come questo:
 * senza il controllo esplicito qui sotto, la stessa pressione di Esc
 * chiuderebbe **anche** il lightbox sotto al dialog, non solo il dialog
 * — bug reale, trovato scrivendo i test della sezione ALBUM (Task 8
 * 7/N), presente fin dal Task 8 2/N (quando `moreOpen` era l'unico caso
 * gestito) e mai notato prima perché nessun test precedente premeva Esc
 * con uno di questi sei dialog aperto. */
const dialogRefs = [deleteDialogOpen, albumDialogOpen, shareDialogOpen, renameDialogOpen, positionDialogOpen, personDialogOpen, tagDialogOpen]

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    if (moreOpen.value) {
      moreOpen.value = false
      return
    }
    const openDialog = dialogRefs.find((dialog) => dialog.value)
    if (openDialog) {
      openDialog.value = false
      return
    }
    emit('close')
    return
  }
  if (e.key === 'i' || e.key === 'I') {
    info.value = !info.value
    if (info.value) void loadPanelData()
    return
  }
  if (e.key === 'f' || e.key === 'F') {
    emit('toggle-favorite')
    return
  }
  if (e.key === 'ArrowLeft') {
    stepTo(prevAsset.value)
    return
  }
  if (e.key === 'ArrowRight') {
    stepTo(nextAsset.value)
  }
}

/** Un solo giro per apertura pannello/cambio foto: metadati effettivi
 * (titolo, posizione), dettaglio con `full_exif` (§19.2 "SCATTO", assente
 * dal prop `asset`) e voti (per la valutazione a stelle) — tre chiamate in
 * parallelo, ciascuna con il proprio esito indipendente: se una fallisce
 * (es. pgvector assente per i voti) le altre due restano valide. */
async function loadPanelData() {
  const sequence = ++panelRequestSequence
  const assetId = props.asset.id
  // `maps.regions` non è caricato da nessun ingresso globale (solo
  // MapView/MapsOfflineView lo fanno) — senza questo, `PlacePickerDialog`
  // vedrebbe sempre `availableRegionIds` vuoto e mostrerebbe l'avviso
  // "mappa non disponibile" anche per regioni già scaricate. A parte,
  // senza `await`: non deve ritardare gli altri tre campi del pannello.
  void maps.loadRegions()
  // Niente giro a vuoto: `asset.faces` (già nel prop, senza fetch) dice se
  // la foto ha volti confermati prima ancora di chiedere i riquadri. In
  // culling (§21.9: "Non richiede... tag, album, volti") tag/album/volti
  // non servono affatto — mai una foto "grezza" di un lotto li ha.
  const needsFaces = !props.isCulling && props.asset.faces.length > 0
  const needsStack = props.asset.raw_kind === 'raw' || props.asset.raw_kind === 'raw+jpeg'
  const [
    metadataResult,
    detailResult,
    flagsResult,
    facesResult,
    tagsResult,
    categoriesResult,
    albumsResult,
    stackResult
  ] = await Promise.allSettled([
    fetchMetadata(assetId),
    fetchAsset(assetId),
    fetchFlags(assetId),
    needsFaces ? fetchFacesForAsset(assetId) : Promise.resolve([]),
    props.isCulling ? Promise.resolve([]) : fetchTagsForAsset(assetId),
    props.isCulling ? Promise.resolve([]) : fetchTags(),
    props.isCulling ? Promise.resolve([]) : fetchAlbumsForAsset(assetId),
    needsStack ? fetchStack(assetId) : Promise.resolve({ stack_id: null, primary_asset_id: null, members: [] })
  ])
  if (sequence !== panelRequestSequence || assetId !== props.asset.id) return
  metadata.value = metadataResult.status === 'fulfilled' ? metadataResult.value : undefined
  detail.value = detailResult.status === 'fulfilled' ? detailResult.value : undefined
  flags.value = flagsResult.status === 'fulfilled' ? flagsResult.value : unvotedFlags
  faces.value = facesResult.status === 'fulfilled' ? facesResult.value : []
  assetTags.value = tagsResult.status === 'fulfilled' ? tagsResult.value : []
  assetAlbums.value = albumsResult.status === 'fulfilled' ? albumsResult.value : []
  stackMembers.value = stackResult.status === 'fulfilled' ? stackResult.value.members : []
  selectedStackMemberId.value = assetId
  categories.value =
    categoriesResult.status === 'fulfilled' ? categoriesResult.value.filter((tag) => tag.kind === 'category') : []
  titleDraft.value = metadata.value?.title ?? ''
  placeName.value = null
  const location = metadata.value?.location
  if (location) {
    maps.reverseGeocode(location.lat, location.lon)
      .then((place) => {
        if (sequence === panelRequestSequence) placeName.value = place?.name ?? null
      })
      .catch(() => { /* best-effort */ })
  }
}

async function saveTitle() {
  const assetId = props.asset.id
  const trimmed = titleDraft.value.trim()
  titleDraft.value = trimmed
  try {
    await patchMetadata(assetId, { title: trimmed === '' ? null : trimmed })
    if (metadata.value && assetId === props.asset.id) {
      metadata.value.title = trimmed === '' ? null : trimmed
    }
  } catch {
    toast.showError(t('viewer.panel.titleError'))
  }
}

/** SP-9: click sulla stella *n* imposta la valutazione a *n*, riclick
 * sulla stessa stella l'azzera — `RatingStars` emette solo `rate(n)`, il
 * toggle è responsabilità del chiamante (stessa cosa già vera in
 * `CullingView.vue`, che però non lo implementa: qui sì, per rispettare
 * §19.3 alla lettera). `setFlags` sostituisce l'intero oggetto voti, quindi
 * si parte sempre da `flags.value` già caricato, mai da un valore vuoto. */
async function rate(n: number) {
  const assetId = props.asset.id
  const current = flags.value ?? unvotedFlags
  const next = current.rating === n ? 0 : n
  try {
    await setFlags(assetId, { ...current, rating: next })
    if (assetId === props.asset.id) flags.value = { ...current, rating: next }
  } catch {
    toast.showError(t('viewer.panel.ratingError'))
  }
}

function personDisplayName(personName: string | null): string {
  return personName ?? t('personPicker.unnamed')
}

/** Un chip persona rappresenta un `person_id`; il volto (`bbox`, e l'id da
 * passare ad `assignFace`/`rejectFace`) va cercato nel dettaglio caricato a
 * parte — `asset.faces` non lo porta (solo nome/id persona, SP-3). */
function faceIdFor(personId: string): string | undefined {
  return faces.value.find((face) => face.person_id === personId)?.id
}

/** §19, animazioni: 0ms all'entrata, 200ms di tolleranza all'uscita — si
 * annullano se nel frattempo si rientra nel chip **o** nel riquadro
 * stesso (da qui i due handler gemelli sui riquadri, non solo sui chip). */
function cancelHideBoxes() {
  if (hideBoxesTimer) {
    clearTimeout(hideBoxesTimer)
    hideBoxesTimer = undefined
  }
}

function showBoxesFor(personId: string) {
  cancelHideBoxes()
  hoveredPersonId.value = personId
}

function scheduleHideBoxes() {
  if (hideBoxesTimer) clearTimeout(hideBoxesTimer)
  hideBoxesTimer = setTimeout(() => {
    hoveredPersonId.value = null
    hideBoxesTimer = undefined
  }, 200)
}

const visibleFaces = computed(() => faces.value.filter((face) => face.person_id === hoveredPersonId.value))

// §19.3/§38.3 controllo 1, "Vai alla persona": chiude il menu, chiude il
// lightbox, passa alla vista Persone e apre il dettaglio — reale da
// quando la rotta `/persons/:id` esiste (Task 16 1/N). Era omesso di
// proposito nel Task 8 (commento di testa del file): "nessuna vista
// Persone esiste ancora... ometterlo è più onesto di un finto toast".
function goToPerson(personId: string) {
  openFaceMenuPersonId.value = null
  emit('close')
  void router.push(`/persons/${personId}`)
}

function openCorrectPerson(personId: string) {
  const faceId = faceIdFor(personId)
  if (!faceId) return
  openFaceMenuPersonId.value = null
  correctingFaceId.value = faceId
  personDialogOpen.value = true
}

async function onPersonPicked(personId: string) {
  const faceId = correctingFaceId.value
  correctingFaceId.value = null
  if (!faceId) return
  try {
    await assignFace(faceId, personId)
    toast.show(t('viewer.panel.personCorrected'))
    void loadPanelData()
  } catch {
    toast.showError(t('personPicker.error'))
  }
}

async function markNotAFace(personId: string) {
  const faceId = faceIdFor(personId)
  openFaceMenuPersonId.value = null
  if (!faceId) return
  try {
    await rejectFace(faceId)
    toast.show(t('viewer.panel.notAFaceToast'))
    void loadPanelData()
  } catch {
    toast.showError(t('personPicker.error'))
  }
}

const confirmedTags = computed(() => assetTags.value.filter((tag) => tag.state === 'confirmed'))
const proposedTags = computed(() => assetTags.value.filter((tag) => tag.state === 'proposed'))

/** Raggruppa i tag confermati per categoria (§19.2 righe 14-15): nessun
 * `TAG_CATEGORIES` lato backend (era una costante del solo prototipo) —
 * ordine alfabetico per nome, "Senza categoria" sempre in fondo. */
const groupedConfirmedTags = computed(() => {
  const groups = new Map<string | null, AssetTagDetail[]>()
  for (const tag of confirmedTags.value) {
    const key = tag.category_id
    const bucket = groups.get(key)
    if (bucket) bucket.push(tag)
    else groups.set(key, [tag])
  }
  const entries = Array.from(groups.entries()).map(([categoryId, tags]) => ({
    categoryId,
    name: categoryId
      ? (categories.value.find((c) => c.id === categoryId)?.name ?? t('viewer.panel.tagNoCategory'))
      : t('viewer.panel.tagNoCategory'),
    tags
  }))
  entries.sort((a, b) => {
    if (a.categoryId === null) return 1
    if (b.categoryId === null) return -1
    return a.name.localeCompare(b.name)
  })
  return entries
})

async function confirmTag(tag: AssetTagDetail) {
  try {
    await confirmTagProposal(tag.id, props.asset.id)
    toast.show(t('viewer.panel.tagConfirmedToast'))
    void loadPanelData()
  } catch {
    toast.showError(t('viewer.panel.tagError'))
  }
}

async function rejectTag(tag: AssetTagDetail) {
  try {
    await rejectTagProposal(tag.id, props.asset.id)
    toast.show(t('viewer.panel.tagRejectedToast'))
    void loadPanelData()
  } catch {
    toast.showError(t('viewer.panel.tagError'))
  }
}

async function removeTag(tag: AssetTagDetail) {
  try {
    await removeConfirmedTag(tag.id, props.asset.id)
    toast.show(t('viewer.panel.tagRemovedToast'))
    void loadPanelData()
  } catch {
    toast.showError(t('viewer.panel.tagError'))
  }
}

/** `TagPickerDialog` applica ogni tocco subito, senza un evento di
 * completamento (§12.3: "l'effetto è immediato: non c'è 'Annulla'") — il
 * pannello si aggiorna alla chiusura, non ad ogni singolo tocco dentro il
 * dialog. Vale anche per `albumDialogOpen`: lo stesso dialog serve sia
 * "+ aggiungi" del pannello (sezione ALBUM) sia "Aggiungi ad album" del
 * menu ⋯ — un solo punto di ricarica per entrambi gli ingressi. */
watch([tagDialogOpen, albumDialogOpen], ([tagOpen, albumOpen], [prevTagOpen, prevAlbumOpen]) => {
  if ((prevTagOpen && !tagOpen) || (prevAlbumOpen && !albumOpen)) void loadPanelData()
})

/** §18.2: i riquadri volto sono posizionati in percentuale rispetto
 * all'immagine **effettivamente disegnata**, non al contenitore — con
 * `object-contain` le due cose divergono ogni volta che il rapporto
 * d'aspetto della foto non è quello del contenitore (lettera-/pillar-
 * boxing). Misurato via `naturalWidth`/`naturalHeight` (dell'`<img>` dopo
 * il suo `load`) e la dimensione dell'elemento (che con `w-full h-full`
 * coincide col contenitore, osservata con `ResizeObserver`). */
const stageImgEl = ref<HTMLImageElement>()
const containerSize = ref({ w: 0, h: 0 })
const naturalSize = ref({ w: 0, h: 0 })
let stageResizeObserver: ResizeObserver | undefined

function onStageImgLoad() {
  if (stageImgEl.value) {
    naturalSize.value = { w: stageImgEl.value.naturalWidth, h: stageImgEl.value.naturalHeight }
  }
}

watch(
  stageImgEl,
  (el) => {
    stageResizeObserver?.disconnect()
    stageResizeObserver = undefined
    if (el) {
      containerSize.value = { w: el.clientWidth, h: el.clientHeight }
      if (typeof ResizeObserver !== 'undefined') {
        stageResizeObserver = new ResizeObserver(() => {
          containerSize.value = { w: el.clientWidth, h: el.clientHeight }
        })
        stageResizeObserver.observe(el)
      }
    }
  },
  { immediate: true }
)
onUnmounted(() => stageResizeObserver?.disconnect())

const imageRect = computed(() => {
  const { w: cw, h: ch } = containerSize.value
  const { w: nw, h: nh } = naturalSize.value
  if (!cw || !ch || !nw || !nh) return null
  const scale = Math.min(cw / nw, ch / nh)
  const renderedW = nw * scale
  const renderedH = nh * scale
  return { offsetX: (cw - renderedW) / 2, offsetY: (ch - renderedH) / 2, renderedW, renderedH }
})

function boxStyle(face: Face) {
  const rect = imageRect.value
  if (!rect) return { opacity: '0' }
  return {
    left: `${rect.offsetX + face.bbox.x * rect.renderedW}px`,
    top: `${rect.offsetY + face.bbox.y * rect.renderedH}px`,
    width: `${face.bbox.w * rect.renderedW}px`,
    height: `${face.bbox.h * rect.renderedH}px`
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKey)
  // Il pannello parte già aperto (§19.8): il giro che prima scattava solo
  // al primo `i`/click sull'icona deve partire subito al montaggio.
  void loadPanelData()
})
onUnmounted(() => window.removeEventListener('keydown', onKey))
watch(
  () => props.asset.id,
  () => {
    panelRequestSequence += 1
    metadata.value = undefined
    detail.value = undefined
    flags.value = undefined
    faces.value = []
    hoveredPersonId.value = null
    assetTags.value = []
    assetAlbums.value = []
    stackMembers.value = []
    selectedStackMemberId.value = null
    placeName.value = null
    titleDraft.value = ''
    if (info.value) void loadPanelData()
  }
)

/** §20.3: `closeMoreAnd(fn)` — il menu chiude e ridisegna **prima**
 * dell'azione, così il dialog che si apre non trova il menu ancora
 * sopra di sé. */
function closeMoreThen(fn: () => void) {
  moreOpen.value = false
  void nextTick(fn)
}

function rotateStub() {
  toast.show(t('viewer.menu.rotateToast'))
}

const DISK_ACTION: Record<DeleteChoice, DiskAction> = {
  index: 'kept',
  trash: 'moved_to_trash',
  disk: 'purged'
}

async function confirmDelete(choice: DeleteChoice) {
  try {
    await deleteAsset(props.asset.id, DISK_ACTION[choice])
    toast.show(t('librarySelectionActions.deleted', { n: 1 }, { plural: 1 }))
    emit('close')
  } catch {
    toast.showError(t('librarySelectionActions.deleteError'))
  }
}

const renameSubtitle = computed(() => t('renameFormula.subtitleSingle', { filename: props.asset.filename }))

/** §19.2 riga 2: "{giorno} {mese} {anno}, ore {H:MM}" — il link verso la
 * cartella/il lotto di provenienza che condivide questa riga nel documento
 * resta debito dichiarato (nessuna rotta per risolvere un nome di cartella
 * dal solo `folder_id` esiste ancora: `GET /folders/{id}` non c'è, solo
 * `tree`/`{id}/children`). */
const dateTimeLabel = computed(() => {
  const iso = props.asset.taken_at_utc
  if (!iso) return ''
  const when = new Date(iso)
  const date = new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'long', year: 'numeric' }).format(when)
  const time = new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit', hour12: false }).format(when)
  return t('viewer.panel.dateTime', { date, time })
})

/** §19.2 riga 8: "{diaframma} · {tempo}s · ISO {iso}" — solo le parti
 * effettivamente presenti nell'exif, unite da " · " (un file senza
 * diaframma noto non deve mostrare "f/undefined"). */
const exposureLine = computed(() => {
  const exif = detail.value?.full_exif
  if (!exif) return ''
  const parts: string[] = []
  if (exif.f_number != null) parts.push(`f/${formatFNumber(exif.f_number)}`)
  if (exif.exposure) parts.push(`${exif.exposure}s`)
  if (exif.iso != null) parts.push(`ISO ${exif.iso}`)
  return parts.join(' · ')
})

function formatFNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}

function formatMB(bytes: number): string {
  return new Intl.NumberFormat(locale.value, { maximumFractionDigits: 1 }).format(bytes / 1_000_000)
}

function selectStackMember(member: StackMember) {
  selectedStackMemberId.value = member.id
}

const cameraLine = computed(() => {
  const exif = detail.value?.full_exif
  if (!exif) return ''
  return [exif.camera_make, exif.camera_model].filter(Boolean).join(' ')
})

const dimensionsLine = computed(() => {
  if (!props.asset.width || !props.asset.height) return ''
  return `${props.asset.width}×${props.asset.height}`
})

/** §19.2 riga 11: "lat, lng" a 4 decimali — non le coordinate grezze del
 * `GeoPointView` (che ne porta molti di più, dal backend). */
const coordsLabel = computed(() => {
  const location = metadata.value?.location
  if (!location) return ''
  return `${location.lat.toFixed(4)}, ${location.lon.toFixed(4)}`
})
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex flex-col bg-black text-[#f2f2f2]"
    role="dialog"
    :aria-label="t('viewer.title')"
  >
    <img
      v-if="prevSrc"
      :src="prevSrc"
      alt=""
      class="hidden"
    >
    <img
      v-if="nextSrc"
      :src="nextSrc"
      alt=""
      class="hidden"
    >

    <div class="flex flex-none items-center justify-between gap-2 px-4 py-3">
      <div class="flex min-w-0 items-center gap-1.5">
        <button
          type="button"
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[#f2f2f2] hover:bg-white/10"
          :aria-label="t('viewer.close')"
          @click="emit('close')"
        >
          <svg
            viewBox="0 0 24 24"
            width="18"
            height="18"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d="M5 5l14 14M19 5L5 19" />
          </svg>
        </button>
        <span class="truncate text-[13px] text-[#d8d8d8]">{{ asset.filename }}</span>
      </div>

      <div class="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-md hover:bg-white/10"
          :class="isFavorite ? 'text-accent' : 'text-[#f2f2f2]'"
          :aria-label="t(isFavorite ? 'viewer.favoriteOn' : 'viewer.favoriteOff')"
          @click="emit('toggle-favorite')"
        >
          <svg
            viewBox="0 0 24 24"
            width="17"
            height="17"
            :fill="isFavorite ? 'currentColor' : 'none'"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path
              d="M12 21s-7.5-4.6-10-9C.3 8.3 2 4 6 4c2.2 0 3.7 1.2 6 3.6C14.3 5.2 15.8 4 18 4c4 0 5.7 4.3 4 8-2.5 4.4-10 9-10 9z"
            />
          </svg>
        </button>
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-md hover:bg-white/10"
          :class="info ? 'text-accent' : 'text-[#f2f2f2]'"
          :aria-label="t('viewer.info')"
          @click="info = !info; if (info) void loadPanelData()"
        >
          <svg
            viewBox="0 0 24 24"
            width="17"
            height="17"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <circle
              cx="12"
              cy="12"
              r="9"
            />
            <path d="M12 11v5.5M12 8v.01" />
          </svg>
        </button>
        <Popover
          v-model:open="moreOpen"
          side="bottom"
          align="end"
        >
          <template #trigger>
            <button
              type="button"
              role="button"
              tabindex="0"
              aria-haspopup="true"
              :aria-expanded="moreOpen"
              :aria-label="t('viewer.moreActions')"
              class="relative flex h-8 w-8 items-center justify-center rounded-md text-[#f2f2f2]
                     hover:bg-white/10 focus-visible:outline-2 focus-visible:outline-offset-2
                     focus-visible:outline-accent"
            >
              <svg
                viewBox="0 0 24 24"
                width="17"
                height="17"
                fill="currentColor"
                aria-hidden="true"
              >
                <circle
                  cx="5"
                  cy="12"
                  r="1.8"
                />
                <circle
                  cx="12"
                  cy="12"
                  r="1.8"
                />
                <circle
                  cx="19"
                  cy="12"
                  r="1.8"
                />
              </svg>
            </button>
          </template>
          <div class="flex w-[188px] flex-col gap-0.5 py-0.5 text-[13px] text-[var(--color-content)]">
            <a
              :href="originalSrc(downloadTarget.id)"
              :download="downloadTarget.filename"
              class="rounded-md px-2.5 py-2 hover:bg-[var(--color-chip-bg)]"
              @click="moreOpen = false"
            >
              {{ t('viewer.menu.download') }}
            </a>
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(rotateStub)"
            >
              {{ t('viewer.menu.rotate') }}
            </button>
            <button
              v-if="!isCulling"
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (shareDialogOpen = true))"
            >
              {{ t('viewer.menu.share') }}
            </button>
            <button
              v-if="!isCulling"
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (albumDialogOpen = true))"
            >
              {{ t('viewer.menu.addToAlbum') }}
            </button>
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (renameDialogOpen = true))"
            >
              {{ t('viewer.menu.rename') }}
            </button>
            <template v-if="!isCulling">
              <div class="my-0.5 h-px bg-[var(--color-border)]" />
              <button
                type="button"
                class="rounded-md px-2.5 py-2 text-left text-danger hover:bg-[var(--color-chip-bg)]"
                @click="closeMoreThen(() => (deleteDialogOpen = true))"
              >
                {{ t('viewer.menu.delete') }}
              </button>
            </template>
          </div>
        </Popover>
      </div>
    </div>

    <div class="flex min-h-0 flex-1">
      <div class="relative min-w-0 flex-1 px-[60px] py-2.5">
        <button
          v-if="prevAsset"
          type="button"
          :aria-label="t('viewer.prev')"
          class="absolute top-1/2 left-2 z-[1] flex h-[38px] w-[38px] -translate-y-1/2 items-center
                 justify-center rounded-full bg-white/[.08] text-[#f2f2f2] hover:bg-white/[.18]"
          @click="stepTo(prevAsset)"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M15 5l-7 7 7 7" />
          </svg>
        </button>
        <div class="relative h-full w-full">
          <img
            ref="stageImgEl"
            :src="src"
            :alt="asset.filename"
            class="m-auto h-full max-h-full w-full max-w-full rounded-md object-contain"
            @load="onStageImgLoad"
          >
          <div
            v-for="face in visibleFaces"
            :key="face.id"
            class="absolute rounded-sm border-2 border-accent transition-[opacity,border-color]"
            :style="{ ...boxStyle(face), transitionDuration: 'var(--duration-fast, .12s)' }"
            @mouseenter="cancelHideBoxes"
            @mouseleave="scheduleHideBoxes"
          />
        </div>
        <button
          v-if="nextAsset"
          type="button"
          :aria-label="t('viewer.next')"
          class="absolute top-1/2 right-2 z-[1] flex h-[38px] w-[38px] -translate-y-1/2 items-center
                 justify-center rounded-full bg-white/[.08] text-[#f2f2f2] hover:bg-white/[.18]"
          @click="stepTo(nextAsset)"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
      <aside
        v-if="info"
        class="w-[296px] shrink-0 overflow-y-auto border-l border-[#232323] bg-[#0c0c0c] p-[18px] text-sm"
      >
        <h3 class="truncate text-[14.5px] font-bold">
          {{ asset.filename }}
        </h3>
        <p
          v-if="dateTimeLabel"
          class="mt-1 text-xs text-[#8f8f92]"
        >
          {{ dateTimeLabel }}
        </p>

        <div class="mt-3.5">
          <label
            for="lbTitleInput"
            class="mb-1 block text-xs text-[#d8d8d8]"
          >
            {{ t('viewer.panel.titleLabel') }}
            <span class="font-normal text-[#7a7a7d]">{{ t('viewer.panel.titleOptional') }}</span>
          </label>
          <input
            id="lbTitleInput"
            v-model="titleDraft"
            type="text"
            :placeholder="t('viewer.panel.titlePlaceholder')"
            class="w-full rounded-md border border-[#262626] bg-[#161616] px-2.5 py-2 text-sm
                   text-[#f0f0f0] placeholder:text-[#7a7a7d] focus-visible:outline-2
                   focus-visible:outline-offset-2 focus-visible:outline-accent"
            @change="saveTitle"
          >
        </div>

        <RatingStars
          class="mt-3"
          :rating="flags?.rating ?? null"
          @rate="rate"
        />

        <div
          v-if="jpegMember"
          class="mt-3 flex gap-1.5"
        >
          <button
            v-if="rawMember"
            type="button"
            class="rounded-md border px-2 py-1 text-[11px]"
            :class="selectedStackMemberId === rawMember.id
              ? 'border-accent bg-[color-mix(in_srgb,var(--color-accent)_16%,transparent)] text-accent'
              : 'border-[#232323] bg-[#1a1a1a] text-[#9a9a9e]'"
            @click="selectStackMember(rawMember)"
          >
            {{ t('viewer.panel.rawChip', { size: formatMB(rawMember.size_bytes) }) }}
          </button>
          <button
            type="button"
            class="rounded-md border px-2 py-1 text-[11px]"
            :class="selectedStackMemberId === jpegMember.id
              ? 'border-accent bg-[color-mix(in_srgb,var(--color-accent)_16%,transparent)] text-accent'
              : 'border-[#232323] bg-[#1a1a1a] text-[#9a9a9e]'"
            @click="selectStackMember(jpegMember)"
          >
            {{ t('viewer.panel.jpegChip', { size: formatMB(jpegMember.size_bytes) }) }}
          </button>
        </div>
        <div
          v-else-if="asset.raw_kind === 'raw'"
          class="mt-3"
        >
          <span class="rounded-md border border-accent bg-[color-mix(in_srgb,var(--color-accent)_16%,transparent)] px-2 py-1 text-[11px] text-accent">
            {{ t('viewer.panel.rawOnlyChip', { size: formatMB(asset.size_bytes) }) }}
          </span>
        </div>

        <section
          v-if="cameraLine || detail?.full_exif?.lens || exposureLine || dimensionsLine"
          class="mt-4"
        >
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.shot') }}
          </h2>
          <dl class="space-y-1 text-[13px]">
            <div
              v-if="cameraLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.camera') }}
              </dt>
              <dd class="truncate text-right">
                {{ cameraLine }}
              </dd>
            </div>
            <div
              v-if="detail?.full_exif?.lens"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.lens') }}
              </dt>
              <dd class="truncate text-right">
                {{ detail.full_exif.lens }}
              </dd>
            </div>
            <div
              v-if="exposureLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.exposure') }}
              </dt>
              <dd class="truncate text-right">
                {{ exposureLine }}
              </dd>
            </div>
            <div
              v-if="dimensionsLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.dimensions') }}
              </dt>
              <dd class="truncate text-right">
                {{ dimensionsLine }}
              </dd>
            </div>
          </dl>
        </section>

        <section class="mt-4">
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.position') }}
          </h2>
          <template v-if="metadata?.location">
            <p
              v-if="placeName"
              class="mb-1 text-content-muted"
            >
              {{ placeName }}
            </p>
            <p class="mb-2 text-xs text-[#8f8f92]">
              {{ coordsLabel }}
            </p>
            <h3 class="mb-2 text-xs font-medium text-[#8f8f92]">
              {{ t('maps.nearbyPhotos') }}
            </h3>
            <MapClusterLayer
              compact
              :center="metadata.location"
              scope="folder"
              :scope-id="asset.folder_id"
              :region-ids="maps.availableRegionIds"
              @asset-click="emit('open-asset', $event)"
            />
          </template>
          <p
            v-else
            class="mb-2 text-[13px] text-[#8f8f92] italic"
          >
            {{ t('viewer.panel.noPosition') }}
          </p>
          <button
            type="button"
            class="mt-2 rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs
                   hover:bg-[#1f1f1f]"
            @click="positionDialogOpen = true"
          >
            {{ t(metadata?.location ? 'viewer.panel.editPosition' : 'viewer.panel.setPosition') }}
          </button>
        </section>

        <section
          v-if="!isCulling && asset.faces.length > 0"
          class="mt-4"
        >
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.people') }}
          </h2>
          <div class="flex flex-wrap gap-1.5">
            <Popover
              v-for="person in asset.faces"
              :key="person.person_id"
              :open="openFaceMenuPersonId === person.person_id"
              side="bottom"
              align="start"
              @update:open="(v) => (openFaceMenuPersonId = v ? person.person_id : null)"
            >
              <template #trigger>
                <button
                  type="button"
                  role="button"
                  tabindex="0"
                  class="rounded-full bg-[#1a1a1a] px-2.5 py-1 text-xs text-[#d8d8d8]"
                  @mouseenter="showBoxesFor(person.person_id)"
                  @mouseleave="scheduleHideBoxes"
                  @focus="showBoxesFor(person.person_id)"
                  @blur="scheduleHideBoxes"
                >
                  {{ personDisplayName(person.person_name) }}
                </button>
              </template>
              <div class="flex w-[260px] flex-col gap-0.5 py-0.5 text-[var(--color-content)]">
                <button
                  type="button"
                  class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
                  @click="goToPerson(person.person_id)"
                >
                  <span class="block text-[13px] font-semibold">{{ t('viewer.panel.faceMenu.goToPerson') }}</span>
                  <span class="block text-[12.5px] text-content-muted">{{ t('viewer.panel.faceMenu.goToPersonHint') }}</span>
                </button>
                <button
                  type="button"
                  class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
                  @click="openCorrectPerson(person.person_id)"
                >
                  <span class="block text-[13px] font-semibold">{{ t('viewer.panel.faceMenu.correct') }}</span>
                  <span class="block text-[12.5px] text-content-muted">{{ t('viewer.panel.faceMenu.correctHint') }}</span>
                </button>
                <button
                  type="button"
                  class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
                  @click="markNotAFace(person.person_id)"
                >
                  <span class="block text-[13px] font-semibold text-danger">{{ t('viewer.panel.faceMenu.notAFace') }}</span>
                  <span class="block text-[12.5px] text-content-muted">{{ t('viewer.panel.faceMenu.notAFaceHint') }}</span>
                </button>
              </div>
            </Popover>
          </div>
        </section>

        <section
          v-if="!isCulling"
          class="mt-4"
        >
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.tags') }}
          </h2>
          <div
            v-for="group in groupedConfirmedTags"
            :key="group.categoryId ?? '__none__'"
            class="mb-2"
          >
            <p class="mb-1 text-[10px] text-[#6b6b6e]">
              {{ group.name }}
            </p>
            <div class="flex flex-wrap gap-1.5">
              <span
                v-for="tag in group.tags"
                :key="tag.id"
                class="flex items-center gap-1.5 rounded-full bg-[#1a1a1a] py-1 pr-1.5 pl-2 text-xs text-[#d8d8d8]"
              >
                <span
                  class="h-2 w-2 rounded-full"
                  :style="{ backgroundColor: tag.color ?? '#6b6b6e' }"
                />
                {{ tag.name }}
                <button
                  type="button"
                  class="opacity-60 hover:opacity-100"
                  :aria-label="t('viewer.panel.tagRemove', { name: tag.name })"
                  @click="removeTag(tag)"
                >
                  ×
                </button>
              </span>
            </div>
          </div>
          <button
            type="button"
            class="rounded-full border border-dashed border-[#3a3a3a] px-2.5 py-1 text-xs text-[#b8b8bc]"
            @click="tagDialogOpen = true"
          >
            {{ t('viewer.panel.tagAdd') }}
          </button>

          <template v-if="proposedTags.length > 0">
            <h2 class="mt-3 mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
              {{ t('viewer.panel.tagsPending') }}
            </h2>
            <div class="flex flex-wrap gap-1.5">
              <span
                v-for="tag in proposedTags"
                :key="tag.id"
                class="flex items-center gap-1.5 rounded-full border border-dashed border-[#3a3a3a]
                       py-1 pr-1.5 pl-2 text-xs text-[#b8b8bc]"
              >
                <span
                  class="h-2 w-2 rounded-full"
                  :style="{ backgroundColor: tag.color ?? '#6b6b6e' }"
                />
                {{ tag.name }}
                <button
                  type="button"
                  class="text-[#6fd08a]"
                  :aria-label="t('viewer.panel.tagConfirm', { name: tag.name })"
                  @click="confirmTag(tag)"
                >
                  ✓
                </button>
                <button
                  type="button"
                  class="text-[#ff8a80]"
                  :aria-label="t('viewer.panel.tagReject', { name: tag.name })"
                  @click="rejectTag(tag)"
                >
                  ×
                </button>
              </span>
            </div>
          </template>
        </section>

        <section
          v-if="!isCulling"
          class="mt-4"
        >
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.albums') }}
          </h2>
          <div class="flex flex-wrap gap-1.5">
            <span
              v-for="album in assetAlbums"
              :key="album.id"
              class="rounded-full bg-[#1a1a1a] px-2.5 py-1 text-xs text-[#d8d8d8]"
            >
              {{ album.name }}
            </span>
            <button
              type="button"
              class="rounded-full border border-dashed border-[#3a3a3a] px-2.5 py-1 text-xs text-[#b8b8bc]"
              @click="albumDialogOpen = true"
            >
              {{ t('viewer.panel.albumAdd') }}
            </button>
          </div>
        </section>

        <section class="mt-4">
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.actions') }}
          </h2>
          <div class="flex flex-wrap gap-2">
            <a
              :href="originalSrc(downloadTarget.id)"
              :download="downloadTarget.filename"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs hover:bg-[#1f1f1f]"
            >
              {{ t('viewer.menu.download') }}
            </a>
            <button
              type="button"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs hover:bg-[#1f1f1f]"
              @click="rotateStub"
            >
              {{ t('viewer.menu.rotate') }}
            </button>
            <button
              v-if="!isCulling"
              type="button"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs hover:bg-[#1f1f1f]"
              @click="shareDialogOpen = true"
            >
              {{ t('viewer.menu.share') }}
            </button>
            <button
              v-if="!isCulling"
              type="button"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs hover:bg-[#1f1f1f]"
              @click="albumDialogOpen = true"
            >
              {{ t('viewer.menu.addToAlbum') }}
            </button>
            <button
              type="button"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs hover:bg-[#1f1f1f]"
              @click="renameDialogOpen = true"
            >
              {{ t('viewer.menu.rename') }}
            </button>
            <button
              v-if="!isCulling"
              type="button"
              class="rounded-md border border-[#262626] bg-[#161616] px-2.5 py-1.5 text-xs text-danger hover:bg-[#1f1f1f]"
              @click="deleteDialogOpen = true"
            >
              {{ t('viewer.menu.delete') }}
            </button>
          </div>
        </section>
      </aside>
    </div>

    <div
      v-if="neighbors.length > 0"
      class="flex flex-none gap-1.5 overflow-x-auto border-t border-[#1c1c1c] px-4 py-2.5"
    >
      <button
        v-for="n in neighbors"
        :key="n.id"
        type="button"
        class="h-[52px] w-[52px] shrink-0 overflow-hidden rounded-[5px]"
        :class="n.id === asset.id ? 'opacity-100 ring-2 ring-accent' : 'opacity-60 hover:opacity-100'"
        @click="stepTo(n)"
      >
        <img
          v-if="n.content_hash"
          :src="mediaThumbSrc(n.content_hash)"
          :alt="n.filename"
          class="h-full w-full object-cover"
        >
      </button>
    </div>

    <DeleteDialog
      v-model:open="deleteDialogOpen"
      :title="t('librarySelectionActions.deleteDialogTitle', { n: 1 })"
      @choose="confirmDelete"
    />
    <AlbumPickerDialog
      v-model:open="albumDialogOpen"
      :assets="[asset]"
    />
    <ShareSelectionDialog
      v-model:open="shareDialogOpen"
      :asset-ids="[asset.id]"
    />
    <RenameFormulaDialog
      v-model:open="renameDialogOpen"
      :assets="[asset]"
      :subtitle="renameSubtitle"
    />
    <PlacePickerDialog
      v-model:open="positionDialogOpen"
      :asset="asset"
      @applied="loadPanelData"
    />
    <PersonPickerDialog
      v-model:open="personDialogOpen"
      @picked="onPersonPicked"
    />
    <TagPickerDialog
      v-model:open="tagDialogOpen"
      :assets="[asset]"
    />
  </div>
</template>

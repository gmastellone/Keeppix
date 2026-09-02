<script setup lang="ts">
// The lightbox: top bar, stage with arrows, filmstrip, info panel and
// ⋯ ("more actions") menu.
//
// The RAW/JPEG switcher works **unlike** the mockup (where "the only
// observable effect is which chip is highlighted", the displayed image
// never changes): here the selection genuinely changes what the stage
// shows and what "Download original" downloads — the point the mockup
// document itself flags as "the backend will need to do something real:
// choose which of the two stack files gets decoded, shown and
// downloaded". Decoding and display already work for free via
// `/media/preview/{hash}` (RAW files already have a derived preview, the
// same reason RAW thumbnails work everywhere in the app); it only needed
// routing the user's choice to the right stack member (`GET
// /assets/{id}/stack`).
//
// A real bug was fixed: per the mockup, "the panel only exists inside the
// lightbox... **and is forced open on every `openLightbox()`**" (and when
// opened from culling) — for a while the panel started **closed**, never
// noticed because every test in this file opened it explicitly with `i`
// before checking any content (masking the defect instead of catching
// it). Fixed: `info` starts `true`, `loadPanelData()` fires from
// `onMounted`, not only from the first `i`/icon click.
//
// The `isCulling` prop ("Differences between the lightbox opened from the
// library vs. from a culling lot") hides PEOPLE/TAG/ALBUM and the "Add to
// album"/"Delete…" actions (panel and ⋯ menu) when the photo comes from a
// lot that hasn't been organized yet — everything else (title, star
// rating, RAW/JPEG, SHOT, LOCATION, filmstrip, arrows, top bar,
// Download/Rotate/Rename) stays identical, per the mockup's table.
// `CullingView.vue` is the only caller with `isCulling`; the four library
// callers (Timeline, Favorites, Search, Map) default `isCulling` to
// `false`.
//
// **Deliberate deviation from the mockup, not a gap**: the mockup
// describes three chip states for a confirmed tag — "applied by AI, never
// reviewed" (reduced opacity, "AI" marker, click-to-confirm) vs.
// "confirmed by a human" (solid, no marker). In the real backend this
// distinction **doesn't exist**: `AssetTagRepo::decide` (`confirm`/
// `reject`) only transitions rows with `state='proposed'` — a
// `state='confirmed'` row has, by construction, already been decided (by
// `confirm()`, which requires an authenticated user, or by a manual
// assignment), regardless of whether its original `source` was `'ai'` or
// `'user'`. Reproducing the "AI, click to confirm" marker on an
// already-confirmed tag would be a button promising an action (a second
// "confirm") that has no real effect — `decide()` is idempotent and does
// nothing if the state already matches. Every confirmed tag therefore has
// **a single appearance**, independent of `source`; the mockup's
// three-way distinction correctly collapses into the backend's two real
// sections: confirmed (done) and proposed (to decide).
//
// **Declared, verified, not glossed-over gaps**:
// - "Share" opens `ShareSelectionDialog.vue` for this single asset — the
//   same mechanism (auto-generated album) already used by the selection
//   bar, see there for why.
// - "Rotate" is a plain CSS transform on the stage `<img>`
//   (`stageTransform`), not a re-encode of the derived thumb/preview/full
//   files: `orientation` (`patchMetadata`) was already writable and
//   already read back by `metadata.value`, it just had no reader on the
//   display side until now — same non-destructive, override-not-rewrite
//   principle as everything else here (XMP sidecars, location overrides).
// - The link to the source folder/lot in the date/time row is omitted:
//   there's no route to resolve a folder's name from just `folder_id`
//   (`GET /folders/{id}` doesn't exist, only `tree`/`{id}/children`) —
//   building one for a single subtitle line is out of scope here.
// - "Go to person" (first item in the chip's menu): it used to be omitted
//   ("no People view exists yet"), built once `/persons/:id` exists — it
//   closes the menu, closes the lightbox (`emit('close')`), navigates.
// - **The face-box menu stays an anchored `Popover`, not a modal
//   dialog** (verified against the mockup: "the face-box menu **doesn't**
//   use this pattern: it's a modal dialog, not an anchored popup menu" —
//   a real deviation, not corrected here: rewriting the container from
//   `Popover` to `Dialog` would also touch the hover/focus behavior of
//   the boxes on the photo (200ms tolerance) for a gain in structural
//   fidelity, not content — the three options with title+description are
//   now all present and real regardless).
// - The people section's "+ add" chip (last chip) isn't built: manually
//   adding a person creates a face **without** an underlying detection
//   (`box:null` in the mockup), but `Face.bbox` in the real domain
//   (`crates/keeppix-domain/src/face.rs`) isn't optional — a real model
//   gap, not a frontend one, already deferred to the faces model work
//   (YuNet+SFace): that's the natural place to revisit the `Face` model
//   once, in one pass.
//
// **Fixed here, not just added**: clicking the black background must
// *not* close the lightbox (explicit — unlike the scrim on modal
// dialogs) — the previous version had `@click.self="emit('close')"` on
// the root container, a behavior never documented for this view. Removed.
//
// **Real bug found and fixed here, present since early on**: `Esc` used
// to close **the lightbox underneath too**, not just the open dialog,
// whenever one of the panel's six dialogs (delete, album, rename,
// position, person, tag) was open — not just the ⋯ menu, the only case
// handled until then. reka-ui's Esc handling runs on `DismissableLayer`,
// an internal mechanism that doesn't coordinate at all with the
// hand-written `window.addEventListener('keydown', onKey)` here: none of
// the six dialogs had ever been taught to `onKey`. Discovered while
// writing the ALBUM section's tests, never previously exercised by any
// test in this file. Fixed with `dialogRefs`, checked before falling
// back to the lightbox's `emit('close')`.
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
    /** The navigation set (arrows + filmstrip), in display order — "all
     * photos in the same folder and month" for the library, already
     * computed by the caller (each view knows its own "neighborhood":
     * `loadedAssets` for Timeline, `filteredAssets` for Favorites/Search).
     * Empty by default: no arrows, no filmstrip — the map popover has no
     * notion of neighborhood and keeps working unchanged. */
    neighbors?: TimelineAsset[]
    isFavorite: boolean
    /** The photo comes from a culling lot, not yet organized into the
     * library — no folder/month/tag/album/faces. Hides the PEOPLE/TAG/
     * ALBUM sections and the "Add to album"/"Delete…" actions (panel and
     * ⋯ menu); everything else (title, stars, RAW/JPEG, SHOT, LOCATION,
     * filmstrip, arrows, top bar, Download/Rotate/Rename) stays
     * identical. `false` by default: the other four callers
     * (Timeline/Favorites/Search/Map) are always library context. */
    isCulling?: boolean
  }>(),
  { neighbors: () => [], isCulling: false }
)
const emit = defineEmits<{
  close: []
  /** Replaces the previous placeholder's two separate `prev`/`next`
   * emits: arrows, filmstrip and keyboard already resolve the target
   * asset from `neighbors`, the caller no longer has to redo the same
   * lookup (`viewingNeighbour`) the old contract forced on it. */
  step: [asset: TimelineAsset]
  'open-asset': [id: string]
  'toggle-favorite': []
}>()
const { t, locale } = useI18n()
const maps = useMapsStore()
const toast = useToastStore()
const router = useRouter()

/** "Forced open on every `openLightbox()`" (and when opened from
 * culling) — not closed by default as in this component's earlier
 * version. `I`/the icon remain the way to close it (and reopen it),
 * unchanged. */
const info = ref(true)
const moreOpen = ref(false)
const albumDialogOpen = ref(false)
const shareDialogOpen = ref(false)
const renameDialogOpen = ref(false)
const deleteDialogOpen = ref(false)
const positionDialogOpen = ref(false)
const metadata = ref<AssetMetadata>()
/** SHOT section: `full_exif` never arrives with the `asset` prop (the
 * grids that pass the asset to the lightbox use `/timeline`/`/search`,
 * which don't compute it) — only `GET /assets/{id}` carries it. */
const detail = ref<TimelineAsset>()
const flags = ref<AssetFlags>()
const placeName = ref<string | null>(null)
const titleDraft = ref('')
/** Detected faces with their bounding box (`bbox`), separate from
 * `asset.faces` (`AssetFaceBadge[]`, only `person_id`/`person_name`,
 * already available from the prop without a fetch) — needed to map each
 * person chip to its matching face(s) on the image. */
const faces = ref<Face[]>([])
/** "Aggiungi persona" — shows every detected face's box at once (not just
 * the one being hovered), including faces the detector found but nobody
 * has confirmed yet (`person_id: null`), so any of them can be clicked
 * to assign a person. Off by default: fetching/rendering every box for
 * every photo, always, is the overhead a per-photo toggle avoids — only
 * on when someone actually asks for it. */
const showAllFaces = ref(false)
const personDialogOpen = ref(false)
const assetTags = ref<AssetTagDetail[]>([])
/** Only `kind === 'category'` from `GET /tags` (the combined tag+category
 * list): needed for each group's name — `AssetTagDetail.category_id`
 * only carries the id. */
const categories = ref<Tag[]>([])
const tagDialogOpen = ref(false)
/** Read-only list of the albums the photo belongs to (manual and dynamic
 * indistinct, `AlbumRepo::for_asset`) — "+ add" reuses
 * `albumDialogOpen`/`AlbumPickerDialog`, already wired for the ⋯ menu:
 * same dialog, two entry points. */
const assetAlbums = ref<AlbumBadge[]>([])
/** The face that "Correct person…" is about to reassign — set when the
 * picker opens, read when the user chooses a person. */
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

/** RAW/JPEG switcher: unlike the mockup ("the only observable effect is
 * which chip is highlighted... the displayed image never changes"), here
 * the selection **genuinely changes what's shown and downloaded** — the
 * mockup document itself flags this as one of the points where "the
 * backend will need to do something real: choose which of the two stack
 * files gets decoded, shown and downloaded". `stackMembers` only arrives
 * for an asset with `raw_kind` `'raw'`/`'raw+jpeg'` (never for a plain
 * JPEG, which has no stack). */
const stackMembers = ref<StackMember[]>([])
const selectedStackMemberId = ref<string | null>(null)
const rawMember = computed(() => stackMembers.value.find((m) => m.raw_kind === 'raw'))
const jpegMember = computed(() => stackMembers.value.find((m) => m.raw_kind === 'jpeg'))
const selectedStackMember = computed(() =>
  stackMembers.value.find((m) => m.id === selectedStackMemberId.value)
)

/** `/media/preview/{hash}` 404s outright for an asset whose original is
 * already at or under `SKIP_PREVIEW_PX` (keeppix-media's `derive.rs`
 * deliberately skips generating a separate — and, for a small original,
 * strictly *larger* — preview file): a small WhatsApp export, a
 * screenshot, anything already close to preview size. Without this, the
 * stage silently shows nothing at all for exactly those assets — the
 * filmstrip thumbnail still works (`thumb.webp` is never skipped), so it
 * reads as "this one photo is broken" rather than what it is, a missing
 * derivative tier. Reset per asset/stack-member so switching away and
 * back retries the real preview instead of getting stuck on the
 * fallback. */
const mainImageErrored = ref(false)
watch(
  () => (selectedStackMember.value ?? props.asset).id,
  () => {
    mainImageErrored.value = false
  }
)

function onMainImageError() {
  mainImageErrored.value = true
}

const src = computed(() => {
  const target = selectedStackMember.value ?? props.asset
  return mainImageErrored.value ? originalSrc(target.id) : previewSrc(target)
})
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

/** Two-level `Esc`: an open ⋯ menu absorbs the first press. Checked here,
 * not left to reka-ui's layering (the lightbox itself isn't a
 * `DialogRoot`/`PopoverRoot`, only the ⋯ menu is): a single global
 * `keydown` needs to know which level to close first before reaching the
 * second. */
/** The six dialogs opened from the panel/menu (delete, album, rename,
 * position, person, tag) are all real reka-ui components with their own
 * Esc handling — but that runs on `DismissableLayer`, a library-internal
 * mechanism that doesn't coordinate at all with a hand-written
 * `window.addEventListener('keydown', ...)` like this one: without the
 * explicit check below, the same Esc press would **also** close the
 * lightbox underneath the dialog, not just the dialog — a real bug, found
 * while writing the ALBUM section's tests, present since `moreOpen` was
 * the only case handled, and never noticed before because no earlier test
 * pressed Esc with one of these six dialogs open. */
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

/** A single round per panel open/photo change: effective metadata (title,
 * position), detail with `full_exif` (SHOT section, absent from the
 * `asset` prop) and votes (for the star rating) — three calls in
 * parallel, each with its own independent outcome: if one fails (e.g.
 * pgvector missing for votes) the other two remain valid. */
async function loadPanelData() {
  const sequence = ++panelRequestSequence
  const assetId = props.asset.id
  // `maps.regions` isn't loaded by any global entry point (only
  // MapView/MapsOfflineView do) — without this, `PlacePickerDialog` would
  // always see `availableRegionIds` empty and show the "map unavailable"
  // warning even for regions already downloaded. Kept separate, without
  // `await`: it must not delay the panel's other three fields.
  void maps.loadRegions()
  // No wasted round trip: `asset.faces` (already in the prop, no fetch)
  // tells us whether the photo has confirmed faces before even asking for
  // the boxes. In culling, tags/albums/faces aren't needed at all — a
  // lot's "raw" photo never has them.
  // `showAllFaces` (the "Aggiungi persona" toggle, below) needs the raw
  // detected faces too — a photo can have faces the detector found but
  // nobody has ever confirmed (person_id null), which never show up in
  // `asset.faces` (confirmed only) and so would otherwise never trigger
  // this fetch at all.
  const needsFaces = !props.isCulling && (props.asset.faces.length > 0 || showAllFaces.value)
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

/** Clicking star *n* sets the rating to *n*, clicking the same star again
 * resets it to 0 — `RatingStars` only emits `rate(n)`, the toggle is the
 * caller's responsibility (same is already true in `CullingView.vue`,
 * which doesn't implement it though: here it does, to follow the spec to
 * the letter). `setFlags` replaces the whole votes object, so it always
 * starts from the already-loaded `flags.value`, never from an empty
 * value. */
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

/** A person chip represents a `person_id`; the face (`bbox`, and the id to
 * pass to `assignFace`/`rejectFace`) has to be looked up in the
 * separately loaded detail — `asset.faces` doesn't carry it (only the
 * person's name/id). */
function faceIdFor(personId: string): string | undefined {
  return faces.value.find((face) => face.person_id === personId)?.id
}

/** Animations: 0ms on enter, 200ms tolerance on leave — cancelled if the
 * pointer re-enters the chip **or** the box itself in the meantime (hence
 * the twin handlers on the boxes, not just on the chips). */
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

const visibleFaces = computed(() =>
  showAllFaces.value ? faces.value : faces.value.filter((face) => face.person_id === hoveredPersonId.value)
)

async function toggleShowAllFaces() {
  showAllFaces.value = !showAllFaces.value
  // The one exception to loadPanelData()'s usual "asset.faces.length > 0"
  // gate (see needsFaces above): turning this on is the explicit ask for
  // exactly the faces that gate would otherwise skip.
  if (showAllFaces.value && faces.value.length === 0) void loadPanelData()
}

/** Clicking a box in "Aggiungi persona" mode: reuses the same
 * `personDialogOpen`/`onPersonPicked` flow "Correct person…" already
 * uses (`assignFace`, unchanged whether the face already had someone or
 * not) — the only difference is going by `face.id` directly instead of
 * looking it up from a `person_id` that might not exist yet. */
function pickPersonForFace(faceId: string) {
  openFaceMenuPersonId.value = null
  correctingFaceId.value = faceId
  personDialogOpen.value = true
}

// "Go to person": closes the menu, closes the lightbox, switches to the
// People view and opens the detail — real now that the `/persons/:id`
// route exists. It used to be deliberately omitted ("no People view
// exists yet... omitting it is more honest than a fake toast").
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

/** Groups confirmed tags by category: no `TAG_CATEGORIES` on the backend
 * (it was a prototype-only constant) — alphabetical order by name, "No
 * category" always last. */
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

/** `TagPickerDialog` applies every touch immediately, with no completion
 * event ("the effect is immediate: there's no 'Undo'") — the panel
 * refreshes on close, not on every single touch inside the dialog. Same
 * for `albumDialogOpen`: the same dialog serves both the panel's "+ add"
 * (ALBUM section) and the ⋯ menu's "Add to album" — a single reload point
 * for both entry points. */
watch([tagDialogOpen, albumDialogOpen], ([tagOpen, albumOpen], [prevTagOpen, prevAlbumOpen]) => {
  if ((prevTagOpen && !tagOpen) || (prevAlbumOpen && !albumOpen)) void loadPanelData()
})

/** Face boxes are positioned as a percentage relative to the image
 * **as actually rendered**, not the container — with `object-contain`
 * the two diverge whenever the photo's aspect ratio doesn't match the
 * container's (letter-/pillar-boxing). Measured via `naturalWidth`/
 * `naturalHeight` (of the `<img>` after its `load`) and the element's
 * size (which with `w-full h-full` matches the container, observed with
 * `ResizeObserver`). */
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
  // The panel starts already open: the round trip that used to fire only
  // on the first `i`/icon click must fire immediately on mount.
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
    showAllFaces.value = false
    assetTags.value = []
    assetAlbums.value = []
    stackMembers.value = []
    selectedStackMemberId.value = null
    placeName.value = null
    titleDraft.value = ''
    if (info.value) void loadPanelData()
  }
)

/** `closeMoreAnd(fn)` — the menu closes and repaints **before** the
 * action, so the dialog that opens doesn't find the menu still on top of
 * it. */
function closeMoreThen(fn: () => void) {
  moreOpen.value = false
  void nextTick(fn)
}

/** A degrees-clockwise override (`asset_overrides.orientation`,
 * previously written but never read anywhere — `patchMetadata` and
 * `metadata.value.orientation` already existed end-to-end, only this
 * button and `stageTransform` below were missing). Deliberately just a
 * plain CSS rotation of the already-derived thumb/preview/full images,
 * not a re-encode of them and never a write to the original file on
 * disk — the same "non-destructive, sidecar/override, never touch the
 * original" principle already used everywhere else in this app (XMP
 * sidecars, location overrides, ...). */
async function rotate() {
  if (!metadata.value) return
  const next = ((metadata.value.orientation ?? 0) + 90) % 360
  const previous = metadata.value.orientation
  metadata.value.orientation = next
  try {
    await patchMetadata(props.asset.id, { orientation: next })
  } catch {
    metadata.value.orientation = previous
    toast.showError(t('viewer.menu.rotateError'))
  }
}

/** Compensates for `object-contain`'s own (unrotated) fit: without this,
 * rotating a landscape photo 90°/270° inside a box still shaped for
 * landscape would overflow it on two sides. `naturalSize`/`containerSize`
 * are the exact inputs `imageRect` (face box positioning) already
 * computes from — reused, not re-measured. */
const stageTransform = computed(() => {
  const rotation = metadata.value?.orientation ?? 0
  if (rotation === 0) return {}
  const { w: cw, h: ch } = containerSize.value
  const { w: nw, h: nh } = naturalSize.value
  if (!cw || !ch || !nw || !nh) return { transform: `rotate(${rotation}deg)` }
  const swapped = rotation === 90 || rotation === 270
  const unrotatedScale = Math.min(cw / nw, ch / nh)
  const rotatedScale = swapped ? Math.min(cw / nh, ch / nw) : unrotatedScale
  const compensate = rotatedScale / unrotatedScale
  return { transform: `rotate(${rotation}deg) scale(${compensate})` }
})

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

/** "{day} {month} {year}, at {H:MM}" — the link to the source
 * folder/lot that shares this row in the mockup remains a declared gap
 * (no route to resolve a folder's name from just `folder_id` exists yet:
 * `GET /folders/{id}` doesn't exist, only `tree`/`{id}/children`). */
const dateTimeLabel = computed(() => {
  const iso = props.asset.taken_at_utc
  if (!iso) return ''
  const when = new Date(iso)
  const date = new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'long', year: 'numeric' }).format(when)
  const time = new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit', hour12: false }).format(when)
  return t('viewer.panel.dateTime', { date, time })
})

/** "{aperture} · {time}s · ISO {iso}" — only the parts actually present
 * in the exif, joined by " · " (a file with no known aperture must not
 * show "f/undefined"). */
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

/** "lat, lng" to 4 decimal places — not the raw coordinates from
 * `GeoPointView` (which carries many more, from the backend). */
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
              @click="closeMoreThen(rotate)"
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
            :style="stageTransform"
            @load="onStageImgLoad"
            @error="onMainImageError"
          >
          <div
            v-for="face in visibleFaces"
            :key="face.id"
            class="absolute rounded-sm border-2 transition-[opacity,border-color]"
            :class="[
              face.person_id ? 'border-accent' : 'border-dashed border-white',
              showAllFaces && 'cursor-pointer hover:border-accent'
            ]"
            :style="{ ...boxStyle(face), transitionDuration: 'var(--duration-fast, .12s)' }"
            :role="showAllFaces ? 'button' : undefined"
            :tabindex="showAllFaces ? 0 : undefined"
            :aria-label="showAllFaces ? t('viewer.panel.addPersonToFace') : undefined"
            @mouseenter="cancelHideBoxes"
            @mouseleave="scheduleHideBoxes"
            @click="showAllFaces && pickPersonForFace(face.id)"
            @keydown.enter="showAllFaces && pickPersonForFace(face.id)"
            @keydown.space.prevent="showAllFaces && pickPersonForFace(face.id)"
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
          v-if="!isCulling"
          class="mt-4"
        >
          <div class="mb-2 flex items-center justify-between gap-2">
            <h2 class="text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
              {{ t('viewer.panel.people') }}
            </h2>
            <button
              type="button"
              class="text-[11px] font-semibold"
              :class="showAllFaces ? 'text-accent' : 'text-content-muted hover:text-content'"
              @click="toggleShowAllFaces"
            >
              {{ showAllFaces ? t('viewer.panel.addPersonDone') : t('viewer.panel.addPerson') }}
            </button>
          </div>
          <p
            v-if="showAllFaces"
            class="mb-2 text-[12px] text-content-muted"
          >
            {{ t('viewer.panel.addPersonHint') }}
          </p>
          <div
            v-if="asset.faces.length > 0"
            class="flex flex-wrap gap-1.5"
          >
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
              @click="rotate"
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

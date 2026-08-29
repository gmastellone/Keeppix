<script setup lang="ts">
// The loading skeleton (core principle: "loading is never a spinner in
// the middle of emptiness: it's a skeleton that already has the SHAPE of
// the content that's arriving"). Two real uses from the prototype
// (keeppix-mockup.html lines 3180-3207), not a generic gray rectangle:
// - `grid`: a justified photo grid (`skelGridHTML`) — same layout as the
//   real tiles (width from aspect ratio, common row height), so when
//   photos arrive they take the skeleton's place without anything
//   shifting.
// - `stream`: the loading timeline (`streamSkeletonPlaceholderHTML`) —
//   two skeleton months, not one: the "title, grid, title, grid" rhythm
//   is part of what's being announced.
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

withDefaults(
  defineProps<{
    variant?: 'grid' | 'stream'
    count?: number
    rowHeight?: number
  }>(),
  { variant: 'grid', count: 24, rowHeight: 150 }
)

// Aspect ratios measured from the prototype (line 3184), not invented:
// cyclical, not a single repeated value, because a skeleton grid made of
// identical squares doesn't resemble a real photo grid.
const SKEL_ASPECTS = [
  1.5, 0.67, 1.5, 1.33, 1.5, 0.75, 1.78, 1.5, 0.67, 1.5, 1.33, 1.5,
  1.5, 0.75, 1.5, 1.78, 1.33, 0.67, 1.5, 1.5, 1.33, 1.5, 0.75, 1.5
]

function aspectFor(index: number): number {
  return SKEL_ASPECTS[index % SKEL_ASPECTS.length]
}

function tileStyle(index: number, rowHeight: number) {
  const ar = aspectFor(index)
  return { height: `${rowHeight}px`, flex: `${ar} 1 ${Math.round(ar * rowHeight)}px` }
}

// Same formula as the prototype (line 3198): between 8 and 16 tiles per
// month, scaled to the approximate count passed by the caller.
function perMonthCount(approxCount: number): number {
  return Math.max(8, Math.min(16, Math.round(approxCount / 2) || 12))
}
</script>

<template>
  <div
    v-if="variant === 'grid'"
    aria-hidden="true"
    class="flex flex-wrap gap-1.5"
  >
    <div
      v-for="i in count"
      :key="i"
      class="skel"
      :style="tileStyle(i - 1, rowHeight)"
    />
  </div>
  <div
    v-else
    role="status"
    :aria-label="t('ui.loadingSkeleton.streamLabel')"
    class="flex flex-col gap-6"
  >
    <div
      v-for="month in 2"
      :key="month"
      class="flex flex-col gap-2.5"
    >
      <div class="flex items-baseline gap-2.5">
        <div
          class="skel"
          style="width: 118px; height: 15px"
        />
        <div
          class="skel"
          style="width: 56px; height: 10px"
        />
      </div>
      <div
        aria-hidden="true"
        class="flex flex-wrap gap-1.5"
      >
        <div
          v-for="i in perMonthCount(count)"
          :key="i"
          class="skel"
          :style="tileStyle(i - 1, rowHeight)"
        />
      </div>
    </div>
  </div>
</template>

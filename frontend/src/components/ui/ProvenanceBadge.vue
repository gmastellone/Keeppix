<script setup lang="ts">
// Automatic vs. human provenance ("a label proposed by recognition and
// one placed by a person must never be indistinguishable in the
// interface, anywhere"). The actual marker (a muted `.lb-tag-chip`): a
// small "AI" badge (9px, weight 700, .8 opacity) with a title explaining
// what it means. Shown for any assignment still of `ai` origin —
// `confirmed`+`ai` or `suggested`+`ai` are both treated as "not yet a
// human signature". The only state that doesn't show it is
// `confirmed`+`human`: the decision lives inside the component rather
// than being left for every caller to remember — that's how "never
// indistinguishable, anywhere" stays true even inside components that
// don't exist yet (tag chips, the face tile, the Review thumbnail).
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{ origin: 'ai' | 'human' }>()
</script>

<template>
  <span
    v-if="origin === 'ai'"
    class="text-[9px] font-bold opacity-80"
    :title="t('ui.provenanceBadge.description')"
    :aria-label="t('ui.provenanceBadge.description')"
  >
    {{ t('ui.provenanceBadge.label') }}
  </span>
</template>

<script setup lang="ts">
// SP-17 — ambito qui: **solo** il meccanismo di commutazione fra
// impalcatura desktop e mobile. La barra a schede che instrada su
// `state.view`, i titoli per vista, il badge culling e il menu account
// (documento funzionale, sezione "Shell mobile") restano fuori: dipendono
// dal router, che è il Task 3 — non ancora scritto. Cablarli ora vorrebbe
// dire inventare convenzioni di instradamento che il router potrebbe poi
// smentire. Questo componente espone solo gli slot che la shell reale
// popolerà quando il router esisterà.
//
// Nota vincolante del piano: **"commuta per larghezza, non per
// interruttore"**. Il prototipo usa `state.device`, un interruttore
// manuale per la demo — `#app.device-mobile` è una classe statica, mai
// legata a una larghezza reale del viewport. Qui invece un vero media
// query. **Nessuna soglia numerica esiste** nel documento funzionale né
// nel mockup (verificato: "sotto una certa larghezza", mai una cifra) —
// 768px è il breakpoint `md` di Tailwind, già lo standard del progetto,
// non un valore misurato sul prototipo. Debito dichiarato, non assunto
// in silenzio.
import { onBeforeUnmount, onMounted, ref } from 'vue'

const isMobile = ref(false)
let query: MediaQueryList | undefined

function sync() {
  if (query) isMobile.value = query.matches
}

onMounted(() => {
  query = window.matchMedia('(max-width: 767px)')
  sync()
  query.addEventListener('change', sync)
})

onBeforeUnmount(() => {
  query?.removeEventListener('change', sync)
})

defineExpose({ isMobile })
</script>

<template>
  <div class="flex h-full">
    <template v-if="!isMobile">
      <slot name="sidebar" />
      <!-- `relative`: ancora di posizionamento per il velo di rilascio del
           sottosistema di caricamento (`UploadDropVeil.vue`, `position:
           absolute; inset:0`) — copre topbar+contenuto, non la sidebar,
           stessa area di `#dropOverlayHost` nel mockup (`.main`, righe
           1433-1446 di keeppix-mockup.html). -->
      <div class="relative flex min-w-0 flex-1 flex-col">
        <slot name="topbar" />
        <slot />
      </div>
    </template>
    <template v-else>
      <div class="flex h-full w-full flex-col overflow-hidden">
        <slot name="mobile-header" />
        <div class="min-h-0 flex-1 overflow-y-auto">
          <slot />
        </div>
        <slot name="mobile-tabbar" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
// SP-16: l'avatar a iniziali (prototipo, `.avatar`, righe 220-229 di
// keeppix-mockup.html). Il commento del prototipo — approvato da
// "Giovanni", non un'omissione di contrasto — spiega la scelta: il testo
// resta **sempre bianco**, mai `--color-accent-text` (scuro in tema
// chiaro), perché le iniziali devono restare leggibili sopra qualunque
// sfondo: il default arancione, un colore hash-based assegnato a un'altra
// persona in condivisione, o un colore scelto dall'utente in Profilo.
// Iniziali calcolate con lo stesso algoritmo del prototipo (riga 4373):
// un carattere per parola del nome, non più di due, maiuscolo.
//
// Il colore non è deciso qui: è una prop. Il componente garantisce che
// la stessa coppia (nome, colore) renda sempre identica — "sincronizzato
// ovunque" (nota vincolante del piano) è la responsabilità di chi legge
// il colore da un'unica fonte (le preferenze dell'utente corrente, o
// l'hash della persona) e lo passa qui, non qualcosa che un componente di
// sola resa possa garantire da solo.
withDefaults(defineProps<{ name: string; color?: string | null; size?: 'sm' | 'lg' }>(), {
  color: null,
  size: 'sm'
})

// Le due sole dimensioni realmente usate nel prototipo: 28px/12px del
// footer utente e della sidebar (`.avatar` CSS di base) e 56px/20px del
// grande avatar in Profilo (riga 7187) — non un rapporto inventato fra
// le due, che nel prototipo non è lineare.
const DIMENSIONS = { sm: { box: 28, font: 12 }, lg: { box: 56, font: 20 } } as const

function initials(name: string): string {
  return name
    .split(' ')
    .filter(Boolean)
    .map((word) => word[0])
    .join('')
    .slice(0, 2)
    .toUpperCase()
}
</script>

<template>
  <span
    class="inline-flex flex-none items-center justify-center rounded-full font-bold text-white"
    :style="{
      width: `${DIMENSIONS[size].box}px`,
      height: `${DIMENSIONS[size].box}px`,
      fontSize: `${DIMENSIONS[size].font}px`,
      background: color ?? 'var(--color-accent)'
    }"
    :aria-label="name"
  >
    {{ initials(name) }}
  </span>
</template>

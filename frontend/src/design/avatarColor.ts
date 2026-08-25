// SP-16 (`docs/ui/documento-funzionale-ui.md:10381-10395`, §29.9 riga
// 9175-9178): il colore dell'avatar dell'utente corrente è una scelta
// personale (8 pastiglie fisse, Profilo); il colore degli **altri**
// utenti in condivisione è "hash-based via hsl(), indipendente da questa
// scelta personale" — nessuna formula esatta nel documento, solo il
// vincolo hash→hsl. Deterministico sullo stesso id/nome: la stessa
// persona ha sempre lo stesso colore, ovunque compaia (§29 "Persone",
// §30 "Condividi selezione").
export function avatarColorFor(seed: string): string {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  }
  const hue = hash % 360
  // Saturazione/luminosità fisse, scelte per restare leggibili con testo
  // bianco sopra (stesso vincolo del preset "Arancione", SP-16) su tutto
  // l'arco delle tonalità.
  return `hsl(${hue}, 55%, 40%)`
}

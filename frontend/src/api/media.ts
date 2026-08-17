/**
 * Costruzione delle URL dei derivati.
 *
 * Le rotte `/media/...` rispondono con `Cache-Control: … immutable`: il
 * browser non rivalida per un anno. Ma l'hash nell'URL indirizza il file
 * **sorgente**, non i byte serviti, e quelli dipendono da come li produciamo.
 * Cambiando la ricetta — formato, qualità, dimensioni, incorporata contro
 * demosaic — lo stesso URL restituisce un'immagine diversa, e chi ha già in
 * cache la vecchia se la tiene per sempre.
 *
 * Appendere la versione della ricetta rende l'URL una vera chiave di
 * contenuto: una ricetta nuova produce URL nuove, e la cache si invalida da
 * sola senza rinunciare a `immutable` (che sulla timeline vale centinaia di
 * richieste di rivalidazione risparmiate).
 *
 * Il valore deve restare uguale a `DERIVATIVE_VERSION` in
 * `crates/keeppix-media/src/derive.rs`: un test in `keeppix-api` lo verifica,
 * quindi cambiarne uno solo fa fallire la build.
 */
export const DERIVATIVE_VERSION = 2

/** Suffisso di invalidazione, uguale per tutti i derivati. */
function v(): string {
  return `?v=${DERIVATIVE_VERSION}`
}

// I percorsi sono scritti **per esteso** in ognuna delle tre funzioni invece
// di essere composti da un parametro (`/media/${kind}/…`). Non è ripetizione
// distratta: `scripts/check-wired.py` verifica che ogni rotta montata abbia un
// consumatore nel frontend cercando la stringa letterale. Componendole
// dinamicamente le rotte diventano invisibili alla guardia, che le segnala
// come mai usate — ed è successo davvero, alla prima stesura di questo file.

/** Miniatura 240 px: griglia della timeline, ricerca, filmstrip. */
export function thumbSrc(hash: string): string {
  return `/media/thumb/${hash}${v()}`
}

/** Anteprima 2048 px: apertura della foto, culling, confronto. */
export function previewSrc(hash: string): string {
  return `/media/preview/${hash}${v()}`
}

/**
 * Rendition ad alta risoluzione per lo zoom del culling. Generata **pigramente**
 * alla prima richiesta: sui RAW può richiedere un demosaic, quindi secondi e
 * non millisecondi.
 */
export function fullSrc(hash: string): string {
  return `/media/full/${hash}${v()}`
}

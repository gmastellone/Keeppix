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

function mediaSrc(kind: 'thumb' | 'preview' | 'full', hash: string): string {
  return `/media/${kind}/${hash}?v=${DERIVATIVE_VERSION}`
}

/** Miniatura 240 px: griglia della timeline, ricerca, filmstrip. */
export function thumbSrc(hash: string): string {
  return mediaSrc('thumb', hash)
}

/** Anteprima 2048 px: apertura della foto, culling, confronto. */
export function previewSrc(hash: string): string {
  return mediaSrc('preview', hash)
}

/**
 * Rendition ad alta risoluzione per lo zoom del culling. Generata **pigramente**
 * alla prima richiesta: sui RAW può richiedere un demosaic, quindi secondi e
 * non millisecondi.
 */
export function fullSrc(hash: string): string {
  return mediaSrc('full', hash)
}

// Fase 11 Task 1 — le costanti di tempo del disegno, misurate riga per riga
// su docs/ui/keeppix-mockup.html, non stimate. La controparte CSS
// (`--duration-*`) vive in src/style.css: qui solo i valori che il codice
// JavaScript/TypeScript deve conoscere (setTimeout, vibrazione, ciclo di
// vita del toast) — un componente che anima via CSS usa `var(--duration-*)`
// direttamente, non questo modulo.

/** Le sole sette durate che il prototipo usa (108/111 casi con la curva
 * `ease`) — mai un numero fuori da questo insieme in un componente nuovo,
 * verificato da `tokens.spec.ts`. */
export const DURATION_MS = {
  xs: 100,
  fast: 120,
  arrow: 150,
  tileIn: 180,
  base: 200,
  theme: 250,
  progress: 300
} as const

export type DurationToken = keyof typeof DURATION_MS

/** SP-6/SP-28/SP-29: il toast compare con un ritardo minimo (evita un
 * lampo su una transizione istantanea), sparisce dal DOM 250ms dopo aver
 * iniziato a dissolversi, e resta visibile una durata diversa secondo la
 * sua natura. */
export const TOAST_SHOW_DELAY_MS = 10
export const TOAST_REMOVE_AFTER_MS = 250
export const TOAST_LIFE_SUCCESS_MS = 2400
/** Vale sia per l'errore sia per la riuscita parziale (SP-28/SP-29) — il
 * prototipo non li distingue in durata, solo nel contenuto. */
export const TOAST_LIFE_ERROR_MS = 4200
/** Con un'azione (es. "Annulla"): il timer si ferma mentre il puntatore è
 * sopra il toast, altrimenti sparirebbe proprio mentre si decide se
 * premerlo. */
export const TOAST_LIFE_WITH_ACTION_MS = 6500

/** Tocco prolungato su mobile: soglia e vibrazione dell'API `Vibration`. */
export const LONG_PRESS_THRESHOLD_MS = 500
export const LONG_PRESS_VIBRATE_MS = 15

/** Pulsazione dell'indicatore di analisi in corso. */
export const ANALYSIS_PULSE_MS = 1400

// Design timing constants, measured directly from the UI mockup rather than
// estimated. The CSS counterpart (`--duration-*`) lives in src/style.css:
// this module only holds the values that JavaScript/TypeScript code needs
// to know (setTimeout, vibration, toast lifecycle) — a component that
// animates via CSS uses `var(--duration-*)` directly, not this module.

/** The only seven durations the prototype uses (almost all with an `ease`
 * curve) — never introduce a number outside this set in a new component;
 * enforced by `tokens.spec.ts`. */
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

/** A toast appears after a minimum delay (avoids a flash on an instant
 * transition), is removed from the DOM 250ms after it starts fading out,
 * and stays visible for a duration that depends on its kind. */
export const TOAST_SHOW_DELAY_MS = 10
export const TOAST_REMOVE_AFTER_MS = 250
export const TOAST_LIFE_SUCCESS_MS = 2400
/** Applies to both errors and partial successes — the prototype doesn't
 * distinguish them by duration, only by content. */
export const TOAST_LIFE_ERROR_MS = 4200
/** With an action (e.g. "Undo"): the timer pauses while the pointer is over
 * the toast, otherwise it could disappear right as the user decides
 * whether to press it. */
export const TOAST_LIFE_WITH_ACTION_MS = 6500

/** Long press on mobile: threshold and vibration for the `Vibration` API. */
export const LONG_PRESS_THRESHOLD_MS = 500
export const LONG_PRESS_VIBRATE_MS = 15

/** Pulse animation for the "analysis in progress" indicator. */
export const ANALYSIS_PULSE_MS = 1400

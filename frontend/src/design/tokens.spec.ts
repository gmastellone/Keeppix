/// <reference types="node" />
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import {
  ANALYSIS_PULSE_MS,
  DURATION_MS,
  LONG_PRESS_THRESHOLD_MS,
  LONG_PRESS_VIBRATE_MS,
  TOAST_LIFE_ERROR_MS,
  TOAST_LIFE_SUCCESS_MS,
  TOAST_LIFE_WITH_ACTION_MS,
  TOAST_REMOVE_AFTER_MS,
  TOAST_SHOW_DELAY_MS
} from '@/design/tokens'

describe('the timing palette matches the prototype, not an estimate', () => {
  it('has exactly the seven durations measured on keeppix-mockup.html', () => {
    expect(DURATION_MS).toEqual({
      xs: 100,
      fast: 120,
      arrow: 150,
      tileIn: 180,
      base: 200,
      theme: 250,
      progress: 300
    })
  })

  it('has the toast lifecycle numbers from the mockup source', () => {
    expect(TOAST_SHOW_DELAY_MS).toBe(10)
    expect(TOAST_REMOVE_AFTER_MS).toBe(250)
    expect(TOAST_LIFE_SUCCESS_MS).toBe(2400)
    expect(TOAST_LIFE_ERROR_MS).toBe(4200)
    expect(TOAST_LIFE_WITH_ACTION_MS).toBe(6500)
  })

  it('has the long-press and analysis-pulse numbers from the mockup source', () => {
    expect(LONG_PRESS_THRESHOLD_MS).toBe(500)
    expect(LONG_PRESS_VIBRATE_MS).toBe(15)
    expect(ANALYSIS_PULSE_MS).toBe(1400)
  })
})

/** Every value of `DURATION_MS`, in the two forms it can appear in CSS
 * (`120ms` or `.12s`). A component that declares `transition-duration` or
 * `animation-duration` outside this set isn't following the timing
 * measured on the prototype — it's inventing a new duration. */
const ALLOWED_CSS_DURATIONS = new Set(
  Object.values(DURATION_MS).flatMap((ms) => [`${ms}ms`, `.${(ms / 1000).toString().slice(2)}s`])
)

// `process.cwd()` instead of `import.meta.url`: Vitest runs tests from the
// package root (`frontend/`), and not every transform environment gives
// `import.meta.url` a real `file://` scheme.
const SRC_ROOT = join(process.cwd(), 'src')

function collectVueFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    const stat = statSync(full)
    if (stat.isDirectory()) {
      collectVueFiles(full, out)
    } else if (entry.endsWith('.vue')) {
      out.push(full)
    }
  }
  return out
}

/** Matches `transition-duration:`/`animation-duration:` and the shorthand
 * `transition:`/`animation:` followed by an explicit time, wherever a ms or
 * s value appears — not just the first one. */
const DURATION_DECLARATION = /(?:transition|animation)(?:-duration)?\s*:\s*[^;]*?(\d+(?:\.\d+)?m?s)/gi

describe('no component declares a duration outside the timing palette', () => {
  const vueFiles = collectVueFiles(SRC_ROOT)

  it('found at least one component to scan (the scanner itself is not a no-op)', () => {
    expect(vueFiles.length).toBeGreaterThan(0)
  })

  for (const file of vueFiles) {
    const relative = file.slice(SRC_ROOT.length + 1)
    it(`${relative} only uses palette durations`, () => {
      const content = readFileSync(file, 'utf-8')
      const offenders: string[] = []
      for (const match of content.matchAll(DURATION_DECLARATION)) {
        const value = match[1]
        // `0s`/`0ms` (no transition) isn't part of the palette by
        // construction, but it isn't an invented duration either: it's
        // "nothing".
        if (value === '0s' || value === '0ms') continue
        if (!ALLOWED_CSS_DURATIONS.has(value)) offenders.push(value)
      }
      expect(offenders).toEqual([])
    })
  }
})

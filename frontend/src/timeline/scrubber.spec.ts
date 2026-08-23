import { describe, expect, it } from 'vitest'

import { monthAbbrev, monthAtOffset, monthFull } from './scrubber'

describe('monthAtOffset', () => {
  // Conteggi deliberatamente sbilanciati: se l'algoritmo pesasse per
  // `count` (comportamento pre-Fase-11, sbagliato — vedi il commento
  // nella sorgente), il mese da 90 scatti occuperebbe quasi tutta la
  // barra. Il documento funzionale (§8.3) lo esclude esplicitamente: i
  // mesi sono equidistanti.
  const buckets = [
    { month: '2024-08', count: 10 },
    { month: '2024-07', count: 90 },
    { month: '2024-06', count: 10 }
  ]

  it('maps the top of the track to the newest month', () => {
    expect(monthAtOffset(buckets, 0, 100)).toBe('2024-08')
  })

  it('maps the bottom of the track to the oldest month', () => {
    expect(monthAtOffset(buckets, 100, 100)).toBe('2024-06')
  })

  it('is equidistant by index, not weighted by count', () => {
    // 3 mesi, indici 0/1/2 su ratio 0..1: il centro esatto (ratio 0.5)
    // arrotonda a round(0.5*2)=1, il mese di mezzo — non quello con più
    // scatti, che qui non è nemmeno al centro dell'elenco.
    expect(monthAtOffset(buckets, 50, 100)).toBe('2024-07')
  })

  it('clamps an offset outside the track instead of returning nothing', () => {
    expect(monthAtOffset(buckets, -20, 100)).toBe('2024-08')
    expect(monthAtOffset(buckets, 500, 100)).toBe('2024-06')
  })

  it('returns undefined for an empty bucket list or a zero-height track', () => {
    expect(monthAtOffset([], 0, 100)).toBeUndefined()
    expect(monthAtOffset(buckets, 0, 0)).toBeUndefined()
  })
})

describe('monthAbbrev', () => {
  it('formats via Intl, localized — not a hardcoded Italian table', () => {
    expect(monthAbbrev('2026-07', 'en')).toBe('Jul')
    expect(monthAbbrev('2026-07', 'it')).toBe('lug')
  })
})

describe('monthFull', () => {
  it('formats the full month name plus year, localized', () => {
    expect(monthFull('2026-07', 'en')).toBe('July 2026')
    expect(monthFull('2026-07', 'it')).toBe('luglio 2026')
  })

  it('does not drift across a month boundary regardless of the runtime timezone', () => {
    // Un mese costruito da un giorno 1 in UTC: se la formattazione non
    // forzasse timeZone:'UTC', un fuso negativo potrebbe leggere il 30 del
    // mese precedente.
    expect(monthFull('2026-01', 'en')).toBe('January 2026')
    expect(monthFull('2025-12', 'en')).toBe('December 2025')
  })
})

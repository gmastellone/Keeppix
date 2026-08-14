import { describe, expect, it } from 'vitest'

// Alias obbligati: `it` collide con la funzione di test `it` importata da
// vitest sopra (stesso identificatore nello stesso scope di modulo — errore
// di parse, non di risoluzione moduli). Il piano usava `it`/`en` diretti.
import enMessages from './en.json'
import itMessages from './it.json'

/// Appiattisce un oggetto annidato in un elenco di chiavi puntate.
function keys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === 'object' && v !== null
      ? keys(v as Record<string, unknown>, `${prefix}${k}.`)
      : [`${prefix}${k}`]
  )
}

describe('traduzioni', () => {
  it('italiano e inglese hanno le stesse chiavi', () => {
    const itKeys = keys(itMessages).sort()
    const enKeys = keys(enMessages).sort()
    expect(itKeys).toEqual(enKeys)
  })

  it('nessuna traduzione è vuota', () => {
    for (const [locale, messages] of [['it', itMessages], ['en', enMessages]] as const) {
      for (const key of keys(messages)) {
        const value = key.split('.').reduce<unknown>(
          (acc, part) => (acc as Record<string, unknown>)[part],
          messages
        )
        expect(value, `${locale}.${key}`).not.toBe('')
      }
    }
  })
})
